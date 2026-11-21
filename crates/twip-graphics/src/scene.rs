//! The display list: a [`Recorder`] takes draw commands and finishes into an immutable, shareable [`Picture`].
//!
//! A picture is the seam between Twip and every backend. It is recorded once (a line, a page, or this frame's overlays),
//! kept by identity, and replayed by the CPU and GPU backends and the PDF writer with a frame transform. Pictures
//! nest: a page picture places its lines' pictures, so an edit records again only the lines it changed. A backend draws
//! a nested picture exactly as it would draw the same commands inline under the composed transform.
//!
//! Recording takes kurbo's `f64` geometry and stores it in `f32`. Strokes are expanded into fills here, with kurbo, so
//! backends only fill. Three shapes are recorded as plain rects, which every backend draws fastest: an axis-aligned
//! rect path, a rounded rect without radii, and an axis-aligned straight stroke with butt or square caps (dashed or not).
//! A solid stroke of an axis-aligned closed rect with miter joins (a table cell's, text box's or frame's outline) is
//! recorded as a frame: the rect grown by half the width, less the rect shrunk by half the width.
//!
//! A picture's bounds are conservative: everything it draws is inside them, in its local space.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use kurbo::{Affine, Cap, Join, PathEl, RoundedRectRadii};

use crate::font::{FontId, FontRegistry, Style, embolden_em};
use crate::geom::{Point, Rect, Transform};
use crate::image::ImageId;
use crate::paint::{BlendMode, Brush, ColorFilter, Extend, ImagePaint, Paint, PremulColor, Sampling, Stop};
use crate::path::{self, Verb};

mod dash;

/// Strokes are expanded in local units, before any zoom. 0.01 is under 0.1 device pixel at Twip's largest scale
/// (400% zoom at dpr 2, about 10.7 pixels per point).
const STROKE_TOLERANCE: f64 = 0.01;

/// The deepest pictures and layers nest, counted together: every backend walks them recursively (and a CPU layer
/// holds a buffer the size of its region), so a document nesting thousands of groups would overflow the stack. The
/// recorder keeps to it: a picture placed deeper isn't, and a layer opened deeper draws its content without its
/// effects. Twip's own nesting (page, line and shape pictures, a few layers) is far below it.
pub(crate) const MAX_DEPTH: u32 = 64;

/// A run of glyphs on one baseline, shaped by Twip: glyph ids with the x of each glyph's origin. The glyph slice is
/// Twip's own layout (`GlyphRun::glyphs`), so recording it copies nothing but the narrowed values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphRun<'a> {
    /// The face.
    pub font: FontId,
    /// The em size, in local units.
    pub size: f64,
    /// The y of the baseline, in local units.
    pub baseline: f64,
    /// (glyph id, x of the glyph's origin).
    pub glyphs: &'a [(u32, f64)],
    /// The styles synthesised for the face.
    pub synthesis: Synthesis,
}

/// Styles a face lacks, drawn by changing its outlines.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Synthesis {
    /// Outlines outset to look bold.
    pub embolden: bool,
    /// Horizontal skew of the outlines: `-0.2` draws a simulated italic.
    pub skew_x: f32,
}

/// What a clip keeps: drawing outside it is discarded until the matching [`Recorder::pop`]. Clips intersect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Clip<'a> {
    /// A rectangle.
    Rect {
        /// The rectangle, in local units.
        rect: kurbo::Rect,
        /// Whether its edges are anti-aliased.
        aa: bool,
    },
    /// The non-zero fill of a path, anti-aliased.
    Path(&'a [PathEl]),
    /// The even-odd fill of a path, anti-aliased.
    EvenOdd(&'a [PathEl]),
}

/// Which points a path's fill covers: those it winds around at all (non-zero), or an odd number of times (even-odd).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    /// Non-zero winding.
    NonZero,
    /// Even-odd winding.
    EvenOdd,
}

/// An offscreen layer: what is drawn until the matching [`Recorder::pop`] is filtered and then composited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer<'a> {
    /// In `0.0..=1.0`.
    pub opacity: f32,
    /// How the layer combines with what is under it.
    pub blend: BlendMode,
    /// Colour filters applied to the layer, in order.
    pub color_filters: &'a [ColorFilter],
    /// Applied in order, in the layer's local units.
    pub filters: &'a [Filter],
    /// The layer's extent in local units: content outside it is clipped.
    pub bounds: Option<kurbo::Rect>,
}

impl Default for Layer<'_> {
    fn default() -> Self {
        Layer { opacity: 1.0, blend: BlendMode::SrcOver, color_filters: &[], filters: &[], bounds: None }
    }
}

/// A layer filter, sized in the layer's local units (a backend scales it by the transform, so it zooms with the
/// content).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    /// Gaussian blur.
    Blur {
        /// The standard deviation.
        sigma: f32,
    },
    /// Morphological erosion: each pixel becomes the minimum within `radius`.
    Erode {
        /// The distance the minimum is taken over.
        radius: f32,
    },
}

/// The largest blur sigma or erode radius drawn, in device pixels (Skia's `kMAX_BLUR_SIGMA`): a layer's buffer grows by
/// at most three times it a side.
pub(crate) const MAX_FILTER_PX: f32 = 532.0;

impl Filter {
    /// The blur's sigma or the erode's radius in device pixels, under a transform stretching lengths by `scale`: at
    /// most [`MAX_FILTER_PX`], and 0 for NaN.
    pub(crate) fn device_size(self, scale: f32) -> f32 {
        let (Filter::Blur { sigma: v } | Filter::Erode { radius: v }) = self;
        let d = v.abs() * scale;
        if d.is_nan() { 0.0 } else { d.min(MAX_FILTER_PX) }
    }
}

/// A picture's identity: unique among the pictures of a process, never reused.
#[allow(dead_code, reason = "an opaque identity: compared and hashed, never read")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PictureId(u64);

/// Recorded drawing: immutable, cheap to clone (a reference count), and `Send + Sync`.
#[derive(Clone)]
pub struct Picture(Arc<Data>);

struct Data {
    id: PictureId,
    /// How deep its pictures and layers nest, itself counted (at most [`MAX_DEPTH`]).
    depth: u32,
    bounds: Option<Rect>,
    /// Whether it, or a picture it places, carries characters (drawn or not), and invisible ones.
    text: bool,
    invisible: bool,
    bytes: usize,
    lists: Lists,
}

impl Picture {
    /// The picture's identifier, unique among pictures.
    pub fn id(&self) -> PictureId {
        self.0.id
    }

    /// What the picture draws, in its local space; `None` when it draws nothing. Glyph ink can reach one device pixel
    /// past it (hinting snaps edges to the pixel grid), so a damaged area taken from it grows by that pixel.
    pub fn bounds(&self) -> Option<kurbo::Rect> {
        self.0.bounds.map(Rect::to_kurbo)
    }

    /// The memory the picture holds, in bytes, not counting the pictures it places (they are shared and count their
    /// own).
    pub fn bytes(&self) -> usize {
        self.0.bytes
    }

    pub(crate) fn local_bounds(&self) -> Option<Rect> {
        self.0.bounds
    }

    pub(crate) fn lists(&self) -> &Lists {
        &self.0.lists
    }

    /// Whether it, or a picture it places, has an invisible run ([`Recorder::invisible_text`]).
    pub(crate) fn has_invisible_text(&self) -> bool {
        self.0.invisible
    }
}

impl fmt::Debug for Picture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Picture").field("id", &self.0.id).field("bounds", &self.0.bounds).field("commands", &self.0.lists.cmds.len()).finish()
    }
}

/// A slice of one of a picture's lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Span {
    pub start: u32,
    pub len: u32,
}

impl Span {
    /// The items pushed onto `list` since it was `start` long.
    fn since<T>(list: &[T], start: usize) -> Span {
        Span { start: start as u32, len: (list.len() - start) as u32 }
    }

    pub fn range(self) -> std::ops::Range<usize> {
        self.start as usize..(self.start + self.len) as usize
    }
}

/// A recorded path: its verbs and its points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PathSpan {
    pub verbs: Span,
    pub points: Span,
}

/// One recorded command. Payloads that don't fit in a few words live in the picture's side lists, by index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Cmd {
    /// A solid axis-aligned rect: most of what a page draws.
    SolidRect {
        rect: Rect,
        color: PremulColor,
        aa: bool,
        blend: BlendMode,
    },
    Rect {
        rect: Rect,
        paint: u32,
    },
    /// Index into `rrects`.
    RRect {
        rrect: u32,
        paint: u32,
    },
    /// Filled by the non-zero rule, or by the even-odd rule (`even_odd`): a field would grow every command.
    Path {
        path: PathSpan,
        paint: u32,
    },
    EvenOddPath {
        path: PathSpan,
        paint: u32,
    },
    /// Index into `dashed`.
    DashedRect {
        dashed: u32,
    },
    /// Index into `frames`.
    Frame {
        frame: u32,
        paint: u32,
    },
    /// Index into `runs`.
    Glyphs {
        run: u32,
        paint: u32,
    },
    /// Index into `images`.
    Image {
        image: u32,
    },
    /// Indexes into `children` and `transforms`.
    Picture {
        child: u32,
        transform: u32,
    },
    PushTransform {
        transform: u32,
    },
    PushClip {
        clip: u32,
    },
    PushLayer {
        layer: u32,
    },
    /// Ends the innermost push. A finished picture's pushes and pops balance.
    Pop,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaintRec {
    pub brush: BrushRec,
    pub blend: BlendMode,
    pub aa: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BrushRec {
    Solid(PremulColor),
    /// `stops` index into `stops`.
    Linear {
        start: Point,
        end: Point,
        stops: Span,
        extend: Extend,
        transform: Transform,
    },
    /// Two-point conical.
    Radial {
        start: Point,
        start_radius: f32,
        end: Point,
        end_radius: f32,
        stops: Span,
        extend: Extend,
        transform: Transform,
    },
    /// Angles in degrees, clockwise from +x.
    Sweep {
        center: Point,
        start_angle: f32,
        end_angle: f32,
        stops: Span,
        extend: Extend,
        transform: Transform,
    },
    Pattern {
        image: ImageId,
        transform: Transform,
        sampling: Sampling,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RunRec {
    pub font: FontId,
    pub size: f32,
    pub baseline: f32,
    /// Index into `glyphs`.
    pub glyphs: Span,
    pub synthesis: Synthesis,
    /// The characters the glyphs draw, as UTF-8 bytes of `text`, and each glyph's cluster (its first byte in them),
    /// in `clusters`: empty unless recorded with [`Recorder::glyphs_text`].
    pub text: Span,
    pub clusters: Span,
    /// Only the characters, drawing nothing ([`Recorder::invisible_text`]): the screen skips the run, a PDF writes
    /// it as invisible text.
    pub invisible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlyphRec {
    /// The id as recorded; read it with [`GlyphRec::id`].
    pub raw: u32,
    pub x: f32,
}

impl GlyphRec {
    /// The glyph id, as [`crate::font::glyph_id`] bounds it (here, not while recording, which stays a copy).
    #[inline]
    pub fn id(&self) -> u32 {
        crate::font::glyph_id(self.raw)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImageRec {
    pub image: ImageId,
    /// In image pixels; `None` is the whole image. Sampling may read just outside it at its edges.
    pub src: Option<Rect>,
    pub dst: Rect,
    pub sampling: Sampling,
    /// Index into `color_filters`.
    pub color_filters: Span,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ClipRec {
    Rect { rect: Rect, aa: bool },
    Path { path: PathSpan, even_odd: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayerRec {
    pub opacity: f32,
    pub blend: BlendMode,
    pub color_filters: Span,
    pub filters: Span,
    pub bounds: Option<Rect>,
    /// What the layer's commands draw, in its local space (before its filters spread it); `None` when they draw
    /// nothing. Glyph ink can reach a device pixel past it, as past a picture's bounds.
    pub content: Option<Rect>,
}

/// A picture's commands and their side lists.
#[derive(Clone, Debug, Default)]
pub(crate) struct Lists {
    pub cmds: Vec<Cmd>,
    pub transforms: Vec<Transform>,
    pub paints: Vec<PaintRec>,
    pub stops: Vec<Stop>,
    /// A rounded rect and its corner radii: top left, top right, bottom right, bottom left.
    pub rrects: Vec<(Rect, [f32; 4])>,
    /// A frame's outer rect and the hole inside it.
    pub frames: Vec<(Rect, Rect)>,
    pub dashed: Vec<Dashed>,
    pub verbs: Vec<Verb>,
    pub points: Vec<Point>,
    pub runs: Vec<RunRec>,
    pub glyphs: Vec<GlyphRec>,
    pub images: Vec<ImageRec>,
    pub clips: Vec<ClipRec>,
    pub layers: Vec<LayerRec>,
    pub color_filters: Vec<ColorFilter>,
    pub filters: Vec<Filter>,
    pub children: Vec<Picture>,
    pub text: String,
    pub clusters: Vec<u32>,
}

impl Lists {
    /// Whether a run here, or in a picture placed here, is invisible.
    pub(crate) fn has_invisible_text(&self) -> bool {
        self.runs.iter().any(|r| r.invisible) || self.children.iter().any(Picture::has_invisible_text)
    }

    fn bytes(&self) -> usize {
        fn size<T>(v: &[T]) -> usize {
            std::mem::size_of_val(v)
        }
        size(&self.cmds)
            + size(&self.transforms)
            + size(&self.paints)
            + size(&self.stops)
            + size(&self.rrects)
            + size(&self.frames)
            + size(&self.dashed)
            + size(&self.verbs)
            + size(&self.points)
            + size(&self.runs)
            + size(&self.glyphs)
            + size(&self.images)
            + size(&self.clips)
            + size(&self.layers)
            + size(&self.color_filters)
            + size(&self.filters)
            + size(&self.children)
            + self.text.len()
            + size(&self.clusters)
    }

    /// Empties every list, keeping their memory for the next recording.
    fn clear(&mut self) {
        let Lists {
            cmds,
            transforms,
            paints,
            stops,
            rrects,
            frames,
            dashed,
            verbs,
            points,
            runs,
            glyphs,
            images,
            clips,
            layers,
            color_filters,
            filters,
            children,
            text,
            clusters,
        } = self;
        cmds.clear();
        transforms.clear();
        paints.clear();
        stops.clear();
        rrects.clear();
        frames.clear();
        dashed.clear();
        verbs.clear();
        points.clear();
        runs.clear();
        glyphs.clear();
        images.clear();
        clips.clear();
        layers.clear();
        color_filters.clear();
        filters.clear();
        children.clear();
        text.clear();
        clusters.clear();
    }
}

/// An open push, with what it changed so `pop` can restore it.
#[derive(Clone, Copy, Debug)]
enum Frame {
    Transform(Affine),
    Clip(Option<kurbo::Rect>),
    Layer { outer_bounds: Option<kurbo::Rect>, outer_clip: Option<kurbo::Rect>, blur: f64, index: usize },
}

/// Records draw commands into a [`Picture`]. A recorder is reused: [`Recorder::finish`] leaves it empty with its memory
/// kept, so recording the next picture allocates nothing until a list outgrows the largest one recorded so far.
#[derive(Debug)]
pub struct Recorder {
    lists: Lists,
    stack: Vec<Frame>,
    /// Local space to the picture's space, for the bounds.
    transform: Affine,
    /// The active clips' bounds in the picture's space; `None` when nothing clips.
    clip: Option<kurbo::Rect>,
    /// What the innermost open layer (or the picture) has drawn so far, in the picture's space.
    bounds: Option<kurbo::Rect>,
    /// Kept between strokes so expanding one reuses the last one's buffers.
    stroker: kurbo::StrokeCtx,
    /// Stroked text's scratch: a glyph's placed outline, and a contour's synthetic bold ring.
    glyph: kurbo::BezPath,
    ring: kurbo::StrokeCtx,
    /// A dashed stroke made safe ([`safe_dashes`]), its pattern kept for the next, and the dashing's held-back elements.
    dashes: kurbo::Stroke,
    dash_stash: Vec<PathEl>,
    /// Layers open now, and the deepest nesting recorded so far ([`MAX_DEPTH`]).
    layers_open: u32,
    depth: u32,
    /// Whether the picture carries characters so far.
    text: bool,
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder::new()
    }
}

impl Recorder {
    /// An empty recorder.
    pub fn new() -> Recorder {
        Recorder {
            lists: Lists::default(),
            stack: Vec::new(),
            transform: Affine::IDENTITY,
            clip: None,
            bounds: None,
            stroker: kurbo::StrokeCtx::default(),
            glyph: kurbo::BezPath::new(),
            ring: kurbo::StrokeCtx::default(),
            dashes: kurbo::Stroke::default(),
            dash_stash: Vec::new(),
            layers_open: 0,
            depth: 1,
            text: false,
        }
    }

    /// Fills `rect` with `paint`.
    pub fn fill_rect<'p>(&mut self, rect: kurbo::Rect, paint: impl Into<Paint<'p>>) {
        let paint = paint.into();
        self.add_bounds(rect.abs());
        let rect = Rect::from_kurbo(rect);
        let cmd = match paint.brush {
            Brush::Solid(c) => Cmd::SolidRect { rect, color: c.premultiply(), aa: paint.aa, blend: paint.blend },
            _ => Cmd::Rect { rect, paint: self.paint(paint) },
        };
        self.lists.cmds.push(cmd);
    }

    /// A rect with rounded corners. Radii that don't fit are scaled down together until they do (as Skia and CSS
    /// scale them); a negative radius is 0.
    pub fn fill_rrect<'p>(&mut self, rect: kurbo::Rect, radii: impl Into<RoundedRectRadii>, paint: impl Into<Paint<'p>>) {
        let rect = rect.abs();
        let radii = fit_radii(rect, radii.into());
        if radii == [0.0; 4] {
            return self.fill_rect(rect, paint);
        }
        self.add_bounds(rect);
        let paint = self.paint(paint.into());
        let rrect = self.lists.rrects.len() as u32;
        self.lists.rrects.push((Rect::from_kurbo(rect), radii));
        self.lists.cmds.push(Cmd::RRect { rrect, paint });
    }

    /// `rect` filled in dashes along its longer side, from its start: `pattern`'s on and off lengths (an even number of
    /// them, at most eight) in units of the rect's thickness on the device, rounded down to whole device pixels and at
    /// least one. Dashes keep their size in device pixels as the scale changes, as Word draws a border's at any zoom;
    /// the PDF writer measures them on a 300 dpi device. A pattern that is empty, odd, longer or all zero, or a brush
    /// other than a solid colour, fills the rect.
    pub fn fill_dashed_rect<'p>(&mut self, rect: kurbo::Rect, pattern: &[u8], paint: impl Into<Paint<'p>>) {
        self.fill_dashed_rect_from(rect, pattern, 0.0, paint);
    }

    /// [`Recorder::fill_dashed_rect`] for the stretch of a longer rect that starts `phase` local units along it: the
    /// pattern carries on from there instead of starting over. A negative or non-finite `phase` is zero.
    pub fn fill_dashed_rect_from<'p>(&mut self, rect: kurbo::Rect, pattern: &[u8], phase: f64, paint: impl Into<Paint<'p>>) {
        let paint = paint.into();
        let valid = pattern.len().is_multiple_of(2) && pattern.len() <= 8 && pattern.iter().any(|&l| l > 0);
        let Brush::Solid(c) = paint.brush else { return self.fill_rect(rect, paint) };
        if !valid {
            return self.fill_rect(rect, paint);
        }
        let rect = rect.abs();
        self.add_bounds(rect);
        let mut lengths = [0; 8];
        lengths[..pattern.len()].copy_from_slice(pattern);
        let dashed = self.lists.dashed.len() as u32;
        self.lists.dashed.push(Dashed {
            rect: Rect::from_kurbo(rect),
            color: c.premultiply(),
            aa: paint.aa,
            blend: paint.blend,
            pattern: lengths,
            len: pattern.len() as u8,
            phase: if phase.is_finite() { phase.max(0.0) as f32 } else { 0.0 },
        });
        self.lists.cmds.push(Cmd::DashedRect { dashed });
    }

    /// The non-zero fill of a path.
    pub fn fill_path<'p>(&mut self, path: &[PathEl], paint: impl Into<Paint<'p>>) {
        self.fill_path_rule(path, FillRule::NonZero, paint);
    }

    /// The fill of a path by `rule`.
    pub fn fill_path_rule<'p>(&mut self, path: &[PathEl], rule: FillRule, paint: impl Into<Paint<'p>>) {
        if let Some(rect) = path::as_rect(path) {
            return self.fill_rect(rect, paint);
        }
        let (verbs, points) = (self.lists.verbs.len(), self.lists.points.len());
        let Some(bounds) = path::push(path, &mut self.lists.verbs, &mut self.lists.points) else { return };
        let path = PathSpan { verbs: Span::since(&self.lists.verbs, verbs), points: Span::since(&self.lists.points, points) };
        self.add_bounds(bounds);
        let paint = self.paint(paint.into());
        self.lists.cmds.push(match rule {
            FillRule::NonZero => Cmd::Path { path, paint },
            FillRule::EvenOdd => Cmd::EvenOddPath { path, paint },
        });
    }

    /// The outline of a path, expanded into a fill here. A width of 0 or less draws nothing. A dash pattern that is
    /// negative somewhere, sums to 0 or past `f64`, or makes more than [`MAX_DASHES`] dashes is ignored: the stroke is
    /// solid.
    pub fn stroke_path<'p>(&mut self, path: &[PathEl], stroke: &kurbo::Stroke, paint: impl Into<Paint<'p>>) {
        // NaN widths draw nothing too.
        if stroke.width.is_nan() || stroke.width <= 0.0 {
            return;
        }
        let mut safe = std::mem::take(&mut self.dashes);
        let changed = safe_dashes(path, stroke, &mut safe);
        self.stroke_safe(path, if changed { &safe } else { stroke }, paint.into());
        self.dashes = safe;
    }

    /// [`Recorder::stroke_path`], its dashes made safe.
    fn stroke_safe(&mut self, path: &[PathEl], stroke: &kurbo::Stroke, paint: Paint<'_>) {
        if let Some((a, b)) = path::as_line(path)
            && (a.x == b.x || a.y == b.y)
            && let Some(square) = flat_caps(stroke)
        {
            if stroke.dash_pattern.is_empty() {
                return self.fill_rect(line_rect(a, b, stroke.width, square), paint);
            }
            let len = a.distance(b);
            dashes(len, &stroke.dash_pattern, stroke.dash_offset, |from, to| {
                let (from, to) = (a.lerp(b, from / len), a.lerp(b, to / len));
                self.fill_rect(line_rect(from, to, stroke.width, square), paint);
            });
            return;
        }
        // Miter joins square a right angle's corner when the limit is over √2 (kurbo's test).
        if stroke.dash_pattern.is_empty()
            && stroke.join == Join::Miter
            && stroke.miter_limit * stroke.miter_limit > 2.0
            && path.last() == Some(&PathEl::ClosePath)
            && let Some(r) = path::as_rect(path)
            && r.width() > 0.0
            && r.height() > 0.0
        {
            let half = stroke.width / 2.0;
            let (outer, hole) = (r.inflate(half, half), r.inflate(-half, -half));
            if hole.width() <= 0.0 || hole.height() <= 0.0 {
                return self.fill_rect(outer, paint);
            }
            self.add_bounds(outer);
            let paint = self.paint(paint);
            let frame = self.lists.frames.len() as u32;
            self.lists.frames.push((Rect::from_kurbo(outer), Rect::from_kurbo(hole)));
            self.lists.cmds.push(Cmd::Frame { frame, paint });
            return;
        }
        let (mut stroker, opts) = (std::mem::take(&mut self.stroker), kurbo::StrokeOpts::default());
        if stroke.dash_pattern.is_empty() {
            kurbo::stroke_with(path.iter().copied(), stroke, &opts, STROKE_TOLERANCE, &mut stroker);
        } else {
            let kurbo::Stroke { width, join, miter_limit, start_cap, end_cap, ref dash_pattern, dash_offset } = *stroke;
            let solid = kurbo::Stroke { width, join, miter_limit, start_cap, end_cap, dash_pattern: Default::default(), dash_offset: 0.0 };
            let mut stash = std::mem::take(&mut self.dash_stash);
            let dashed = dash::dash(path.iter().copied(), dash_offset, dash_pattern, &mut stash);
            kurbo::stroke_with(dashed, &solid, &opts, STROKE_TOLERANCE, &mut stroker);
            self.dash_stash = stash;
        }
        self.fill_path(stroker.output().elements(), paint);
        self.stroker = stroker;
    }

    /// A straight stroke from `from` to `to`.
    pub fn line<'p>(&mut self, from: kurbo::Point, to: kurbo::Point, stroke: &kurbo::Stroke, paint: impl Into<Paint<'p>>) {
        self.stroke_path(&[PathEl::MoveTo(from), PathEl::LineTo(to)], stroke, paint);
    }

    /// A run of glyphs. `fonts` gives each glyph's outline box, so the picture's bounds hold the run's ink, to within
    /// a device pixel (where hinting snaps edges to the grid).
    pub fn glyphs<'p>(&mut self, fonts: &FontRegistry, run: &GlyphRun<'_>, paint: impl Into<Paint<'p>>) {
        self.glyphs_text(fonts, run, "", &[], paint);
    }

    /// [`Recorder::glyphs`], with the characters the glyphs draw: `text`, and each glyph's cluster (the byte in `text`
    /// its characters start at; one per glyph). A PDF's text extracts as them; the screen draws the glyphs alone.
    /// Clusters that don't match the glyphs, or fall outside `text`, are dropped (the text is kept, for the run).
    pub fn glyphs_text<'p>(&mut self, fonts: &FontRegistry, run: &GlyphRun<'_>, text: &str, clusters: &[u32], paint: impl Into<Paint<'p>>) {
        self.run(fonts, run, text, clusters, paint.into(), false);
    }

    /// The characters `text` at a run's glyphs (clusters as [`Recorder::glyphs_text`] takes them), drawing nothing:
    /// the text of glyphs drawn some other way, such as [`Recorder::stroke_glyphs`]. The screen skips it; a PDF writes
    /// it as invisible text, so the characters extract, search and tag as the run's.
    pub fn invisible_text(&mut self, fonts: &FontRegistry, run: &GlyphRun<'_>, text: &str, clusters: &[u32]) {
        self.run(fonts, run, text, clusters, crate::paint::Color::BLACK.into(), true);
    }

    fn run(&mut self, fonts: &FontRegistry, run: &GlyphRun<'_>, text: &str, clusters: &[u32], paint: Paint<'_>, invisible: bool) {
        if run.glyphs.is_empty() || !(run.size > 0.0 && run.size.is_finite()) {
            return;
        }
        let start = self.lists.text.len();
        self.lists.text.push_str(text);
        self.text |= !text.is_empty();
        let text_span = Span::since(self.lists.text.as_bytes(), start);
        let start = self.lists.clusters.len();
        if clusters.len() == run.glyphs.len() && clusters.iter().all(|&c| (c as usize) < text.len().max(1)) {
            self.lists.clusters.extend_from_slice(clusters);
        }
        let cluster_span = Span::since(&self.lists.clusters, start);
        let start = self.lists.glyphs.len();
        self.lists.glyphs.extend(run.glyphs.iter().map(|&(id, x)| GlyphRec { raw: id, x: x as f32 }));
        let style = Style { skew_x: run.synthesis.skew_x, bold: if run.synthesis.embolden { embolden_em(run.size as f32) } else { 0.0 } };
        let mut ink: Option<kurbo::Rect> = None;
        let mut origins = run.glyphs.iter();
        fonts.boxes(run.font, run.glyphs.iter().map(|g| g.0), |_, em| {
            let x = origins.next().map_or(0.0, |g| g.1);
            if let Some(em) = em {
                let b = style.ink(em);
                let b = kurbo::Rect::new(x + b.x0 * run.size, run.baseline + b.y0 * run.size, x + b.x1 * run.size, run.baseline + b.y1 * run.size);
                ink = Some(ink.map_or(b, |i| i.union(b)));
            }
        });
        if let Some(ink) = ink {
            self.add_bounds(ink);
        }
        let glyphs = Span::since(&self.lists.glyphs, start);
        let rec = RunRec {
            font: run.font,
            size: run.size as f32,
            baseline: run.baseline as f32,
            glyphs,
            synthesis: run.synthesis,
            text: text_span,
            clusters: cluster_span,
            invisible,
        };
        let run = self.lists.runs.len() as u32;
        self.lists.runs.push(rec);
        let paint = self.paint(paint);
        self.lists.cmds.push(Cmd::Glyphs { run, paint });
    }
    /// The outlines of a run's glyphs stroked with `stroke`: hollow text. Each glyph's outline is the edge its fill
    /// would have (skewed for synthetic italic, grown by half synthetic bold's width), expanded into a fill as
    /// [`Recorder::stroke_path`] expands any path, so every backend draws it as a path (and a PDF keeps no text for
    /// it). Glyphs without an outline (spaces, colour and bitmap glyphs) draw nothing. `fonts` reads each face's
    /// glyph outline once and keeps it.
    pub fn stroke_glyphs<'p>(&mut self, fonts: &FontRegistry, run: &GlyphRun<'_>, stroke: &kurbo::Stroke, paint: impl Into<Paint<'p>>) {
        if stroke.width.is_nan() || stroke.width <= 0.0 {
            return;
        }
        self.glyph_outlines(fonts, run, paint.into(), Some(stroke));
    }

    /// A run's glyphs filled as paths: their outlines as [`Recorder::glyphs`] fills them, unhinted and without
    /// characters, for drawing that is no text (a PDF extracts nothing from it). Glyphs without an outline draw nothing.
    pub fn glyph_paths<'p>(&mut self, fonts: &FontRegistry, run: &GlyphRun<'_>, paint: impl Into<Paint<'p>>) {
        self.glyph_outlines(fonts, run, paint.into(), None);
    }

    /// Each of a run's glyph outlines, placed, then stroked or (`stroke: None`) filled.
    fn glyph_outlines(&mut self, fonts: &FontRegistry, run: &GlyphRun<'_>, paint: Paint<'_>, stroke: Option<&kurbo::Stroke>) {
        if run.glyphs.is_empty() || !(run.size > 0.0 && run.size.is_finite()) {
            return;
        }
        let style = Style { skew_x: run.synthesis.skew_x, bold: if run.synthesis.embolden { embolden_em(run.size as f32) } else { 0.0 } };
        let (mut glyph, mut ring) = (std::mem::take(&mut self.glyph), std::mem::take(&mut self.ring));
        fonts.outlines(run.font, run.glyphs.iter().map(|g| g.0), |k, outline| {
            let map = Affine::translate((run.glyphs[k].1, run.baseline)) * Affine::scale(run.size) * style.skew();
            glyph.truncate(0);
            match stroke {
                Some(stroke) => {
                    if style.bold > 0.0 {
                        bold_rings(outline, style, &mut ring, Ring::Edge, |els, _| glyph.extend(els.iter().map(|&e| map * e)));
                    } else {
                        glyph.extend(outline.iter().map(|e| map * e));
                    }
                    self.stroke_path(glyph.elements(), stroke, paint);
                }
                None => {
                    glyph.extend(outline.iter().map(|e| map * e));
                    if style.bold > 0.0 {
                        bold_rings(outline, style, &mut ring, Ring::Whole, |els, reversed| push_mapped(&mut glyph, els, map, reversed));
                    }
                    self.fill_path(glyph.elements(), paint);
                }
            }
        });
        (self.glyph, self.ring) = (glyph, ring);
    }

    /// `image`, or its `src` rect (in image pixels), drawn into `dst`.
    pub fn image(&mut self, image: ImageId, src: Option<kurbo::Rect>, dst: kurbo::Rect, paint: &ImagePaint<'_>) {
        self.add_bounds(dst.abs());
        let start = self.lists.color_filters.len();
        self.lists.color_filters.extend_from_slice(paint.color_filters);
        let color_filters = Span::since(&self.lists.color_filters, start);
        let rec = ImageRec { image, src: src.map(Rect::from_kurbo), dst: Rect::from_kurbo(dst), sampling: paint.sampling, color_filters };
        let image = self.lists.images.len() as u32;
        self.lists.images.push(rec);
        self.lists.cmds.push(Cmd::Image { image });
    }

    /// Places `picture`, mapped by `transform`. The picture is shared, not copied. One that draws nothing is placed
    /// only for the characters it carries (a PDF's text).
    pub fn picture(&mut self, picture: &Picture, transform: Affine) {
        let bounds = picture.bounds();
        if bounds.is_none() && !picture.0.text {
            return;
        }
        let depth = 1 + self.layers_open + picture.0.depth;
        if depth > MAX_DEPTH {
            return;
        }
        self.depth = self.depth.max(depth);
        self.text |= picture.0.text;
        if let Some(bounds) = bounds {
            self.add_bounds(transform.transform_rect_bbox(bounds));
        }
        let child = self.lists.children.len() as u32;
        self.lists.children.push(picture.clone());
        let transform = self.transform_index(transform);
        self.lists.cmds.push(Cmd::Picture { child, transform });
    }

    /// Maps what is drawn until the matching [`Recorder::pop`] by `transform`, after the transforms already pushed.
    pub fn push_transform(&mut self, transform: Affine) {
        self.stack.push(Frame::Transform(self.transform));
        self.transform *= transform;
        let transform = self.transform_index(transform);
        self.lists.cmds.push(Cmd::PushTransform { transform });
    }

    /// Intersects the clip with `clip` until the matching [`Recorder::pop`].
    pub fn push_clip(&mut self, clip: Clip<'_>) {
        let (bounds, rec) = match clip {
            Clip::Rect { rect, aa } => (rect.abs(), ClipRec::Rect { rect: Rect::from_kurbo(rect), aa }),
            Clip::Path(path) | Clip::EvenOdd(path) => {
                let (verbs, points) = (self.lists.verbs.len(), self.lists.points.len());
                // An empty path keeps nothing.
                let bounds = path::push(path, &mut self.lists.verbs, &mut self.lists.points).unwrap_or(kurbo::Rect::ZERO);
                let (verbs, points) = (Span::since(&self.lists.verbs, verbs), Span::since(&self.lists.points, points));
                (bounds, ClipRec::Path { path: PathSpan { verbs, points }, even_odd: matches!(clip, Clip::EvenOdd(_)) })
            }
        };
        self.stack.push(Frame::Clip(self.clip));
        self.clip = Some(self.clipped(self.transform.transform_rect_bbox(bounds)));
        let clip = self.lists.clips.len() as u32;
        self.lists.clips.push(rec);
        self.lists.cmds.push(Cmd::PushClip { clip });
    }

    /// Starts a layer until the matching [`Recorder::pop`].
    pub fn push_layer(&mut self, layer: &Layer<'_>) {
        if 1 + self.layers_open + 1 > MAX_DEPTH {
            return self.push_transform(Affine::IDENTITY);
        }
        self.layers_open += 1;
        self.depth = self.depth.max(1 + self.layers_open);
        let start = self.lists.color_filters.len();
        self.lists.color_filters.extend_from_slice(layer.color_filters);
        let color_filters = Span::since(&self.lists.color_filters, start);
        let start = self.lists.filters.len();
        self.lists.filters.extend_from_slice(layer.filters);
        let filters = Span::since(&self.lists.filters, start);
        // A blur spreads the layer's content by about 3 sigma, in the picture's space.
        let sigma: f64 = layer.filters.iter().map(|f| if let Filter::Blur { sigma } = f { sigma.abs() as f64 } else { 0.0 }).sum();
        let blur = 3.0 * sigma * max_scale(self.transform);
        let index = self.lists.layers.len();
        self.stack.push(Frame::Layer { outer_bounds: self.bounds.take(), outer_clip: self.clip, blur, index });
        if let Some(b) = layer.bounds {
            self.clip = Some(self.clipped(self.transform.transform_rect_bbox(b.abs())));
        }
        let rec = LayerRec {
            opacity: layer.opacity.clamp(0.0, 1.0),
            blend: layer.blend,
            color_filters,
            filters,
            bounds: layer.bounds.map(Rect::from_kurbo),
            content: None,
        };
        let layer = self.lists.layers.len() as u32;
        self.lists.layers.push(rec);
        self.lists.cmds.push(Cmd::PushLayer { layer });
    }

    /// Ends the innermost transform, clip or layer.
    pub fn pop(&mut self) {
        debug_assert!(!self.stack.is_empty(), "a pop without a push");
        let Some(frame) = self.stack.pop() else { return };
        match frame {
            Frame::Transform(outer) => self.transform = outer,
            Frame::Clip(outer) => self.clip = outer,
            Frame::Layer { outer_bounds, outer_clip, blur, index } => {
                self.layers_open -= 1;
                self.clip = outer_clip;
                // The content back in the layer's local space (the transform is the one it was pushed under). A
                // transform that collapses space draws nothing; one too extreme to invert leaves the content unbounded.
                let det = self.transform.determinant();
                self.lists.layers[index].content = self.bounds.filter(|_| det != 0.0).map(|b| {
                    let local = self.transform.inverse().transform_rect_bbox(b);
                    if local.is_finite() { Rect::from_kurbo(local) } else { Rect { x0: f32::MIN, y0: f32::MIN, x1: f32::MAX, y1: f32::MAX } }
                });
                let inner = self.bounds.map(|b| b.inflate(blur, blur));
                self.bounds = outer_bounds;
                if let Some(b) = inner {
                    self.add_picture_bounds(b);
                }
            }
        }
        self.lists.cmds.push(Cmd::Pop);
    }

    /// The picture, with any pushes still open ended. The recorder is left empty, ready for the next picture.
    pub fn finish(&mut self) -> Picture {
        while !self.stack.is_empty() {
            self.pop();
        }
        // Cloning copies each list into memory of exactly its length; the recorder keeps its larger buffers.
        let lists = self.lists.clone();
        self.lists.clear();
        let bounds = self.bounds.take().map(Rect::from_kurbo);
        self.transform = Affine::IDENTITY;
        self.clip = None;
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = PictureId(NEXT.fetch_add(1, Ordering::Relaxed));
        let bytes = std::mem::size_of::<Data>() + lists.bytes();
        let depth = std::mem::replace(&mut self.depth, 1);
        let text = std::mem::take(&mut self.text);
        let invisible = lists.has_invisible_text();
        Picture(Arc::new(Data { id, depth, bounds, text, invisible, bytes, lists }))
    }

    fn paint(&mut self, paint: Paint<'_>) -> u32 {
        let brush = match paint.brush {
            Brush::Solid(c) => BrushRec::Solid(c.premultiply()),
            Brush::Linear { start, end, stops, extend, transform } => BrushRec::Linear {
                start: start.into(),
                end: end.into(),
                stops: self.stops(stops),
                extend,
                transform: Transform::from_kurbo(transform),
            },
            Brush::Radial { start, start_radius, end, end_radius, stops, extend, transform } => BrushRec::Radial {
                start: start.into(),
                start_radius: start_radius as f32,
                end: end.into(),
                end_radius: end_radius as f32,
                stops: self.stops(stops),
                extend,
                transform: Transform::from_kurbo(transform),
            },
            Brush::Sweep { center, start_angle, end_angle, stops, extend, transform } => BrushRec::Sweep {
                center: center.into(),
                start_angle: start_angle as f32,
                end_angle: end_angle as f32,
                stops: self.stops(stops),
                extend,
                transform: Transform::from_kurbo(transform),
            },
            Brush::Pattern { image, transform, sampling } => BrushRec::Pattern { image, transform: Transform::from_kurbo(transform), sampling },
        };
        let index = self.lists.paints.len() as u32;
        self.lists.paints.push(PaintRec { brush, blend: paint.blend, aa: paint.aa });
        index
    }

    /// Records `stops` in offset order. The sort is stable (equal offsets keep their order: a hard colour change) and
    /// allocates nothing.
    fn stops(&mut self, stops: &[Stop]) -> Span {
        let start = self.lists.stops.len();
        self.lists.stops.extend_from_slice(stops);
        let added = &mut self.lists.stops[start..];
        for i in 1..added.len() {
            let mut j = i;
            while j > 0 && added[j - 1].offset > added[j].offset {
                added.swap(j - 1, j);
                j -= 1;
            }
        }
        Span::since(&self.lists.stops, start)
    }

    fn transform_index(&mut self, transform: Affine) -> u32 {
        let index = self.lists.transforms.len() as u32;
        self.lists.transforms.push(Transform::from_kurbo(transform));
        index
    }

    /// Adds a draw's bounds, in its local space, to what the picture draws.
    fn add_bounds(&mut self, local: kurbo::Rect) {
        self.add_picture_bounds(self.transform.transform_rect_bbox(local));
    }

    fn add_picture_bounds(&mut self, b: kurbo::Rect) {
        let b = self.clipped(b);
        if b.area() > 0.0 {
            self.bounds = Some(self.bounds.map_or(b, |a| a.union(b)));
        }
    }

    fn clipped(&self, b: kurbo::Rect) -> kurbo::Rect {
        self.clip.map_or(b, |c| c.intersect(b))
    }
}

/// Radii in `rect`'s corners (top left, top right, bottom right, bottom left), scaled down together when two on one
/// side would overlap.
fn fit_radii(rect: kurbo::Rect, radii: RoundedRectRadii) -> [f32; 4] {
    let r = [radii.top_left, radii.top_right, radii.bottom_right, radii.bottom_left].map(|v| if v > 0.0 { v } else { 0.0 });
    let (w, h) = (rect.width(), rect.height());
    let fit = |side: f64, a: f64, b: f64| if a + b > side { side / (a + b) } else { 1.0 };
    let k = fit(w, r[0], r[1]).min(fit(w, r[3], r[2])).min(fit(h, r[0], r[3])).min(fit(h, r[1], r[2]));
    r.map(|v| (v * k) as f32)
}

/// Whether both caps are flat (butt, `Some(false)`) or square (`Some(true)`); `None` for anything else.
fn flat_caps(stroke: &kurbo::Stroke) -> Option<bool> {
    match (stroke.start_cap, stroke.end_cap) {
        (Cap::Butt, Cap::Butt) => Some(false),
        (Cap::Square, Cap::Square) => Some(true),
        _ => None,
    }
}

/// The most dash pattern entries a stroke walks: a pattern finer than this along its path (zero-length entries count)
/// draws the stroke solid, where it would take time and memory without bound.
const MAX_DASHES: f64 = 200_000.0;

/// A rect filled in dashes measured on the device ([`Recorder::fill_dashed_rect`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Dashed {
    pub rect: Rect,
    pub color: PremulColor,
    pub aa: bool,
    pub blend: BlendMode,
    pattern: [u8; 8],
    len: u8,
    phase: f32,
}

impl Dashed {
    /// Each dash under `t` (local units to device pixels) that reaches `view` (device pixels; under rotation or skew,
    /// the whole rect), as a rect in local units. `false` when there would be more than [`MAX_DASHES`] or `t` is
    /// degenerate: the rect is then drawn solid.
    pub(crate) fn each(&self, t: Transform, view: Rect, mut dash: impl FnMut(Rect)) -> bool {
        let r = self.rect;
        let along_x = r.x1 - r.x0 >= r.y1 - r.y0;
        let (ax, ay, cx, cy) = if along_x { (t.a, t.b, t.c, t.d) } else { (t.c, t.d, t.a, t.b) };
        let (along, across) = (f64::from(ax).hypot(f64::from(ay)), f64::from(cx).hypot(f64::from(cy)));
        let (s0, s1, thick) = if along_x { (r.x0, r.x1, r.y1 - r.y0) } else { (r.y0, r.y1, r.x1 - r.x0) };
        let (s0, s1) = (f64::from(s0), f64::from(s1));
        // A width on whole pixels but for `f32` rounding (6 points at 300 dpi: 24.999998) keeps them.
        let unit = (f64::from(thick) * across + 1.0 / 256.0).floor().max(1.0) / along;
        let pattern = &self.pattern[..self.len as usize];
        let sum: u32 = pattern.iter().map(|&l| u32::from(l)).sum();
        let period = unit * f64::from(sum);
        if !period.is_finite() || period <= 0.0 {
            return false;
        }
        // What of the rect the view sees, along it.
        let (mut v0, mut v1) = (s0, s1);
        if t.is_scale_translate() {
            let (k, o, lo, hi) = if along_x { (t.a, t.e, view.x0, view.x1) } else { (t.d, t.f, view.y0, view.y1) };
            let (a, b) = ((f64::from(lo) - f64::from(o)) / f64::from(k), (f64::from(hi) - f64::from(o)) / f64::from(k));
            (v0, v1) = (v0.max(a.min(b)), v1.min(a.max(b)));
        }
        if v1 <= v0 {
            return true;
        }
        // The pattern's start: `phase` before the rect's.
        let o = s0 - f64::from(self.phase);
        let (k0, k1) = (((v0 - o) / period).floor(), ((v1 - o) / period).ceil());
        if (k1 - k0) * pattern.len() as f64 > MAX_DASHES {
            return false;
        }
        for k in k0 as u64..k1 as u64 {
            let mut at = k * u64::from(sum);
            for (i, &l) in pattern.iter().enumerate() {
                let (from, to) = (o + at as f64 * unit, o + (at + u64::from(l)) as f64 * unit);
                at += u64::from(l);
                let (from, to) = (from.max(s0) as f32, to.min(s1) as f32);
                if i % 2 == 0 && to > from {
                    dash(if along_x { Rect { x0: from, x1: to, ..r } } else { Rect { y0: from, y1: to, ..r } });
                }
            }
        }
        true
    }
}

/// What [`bold_rings`] passes on of each contour's synthetic bold ring.
#[derive(Clone, Copy, PartialEq)]
enum Ring {
    /// The ring, wound as the glyph is: with the outline, its non-zero fill is the bold glyph.
    Whole,
    /// The ring's side away from the glyph's ink (an outer contour's outer side, a hole's inner side): the bold
    /// glyph's edge.
    Edge,
}

/// Each contour of `outline` (em units)'s synthetic bold ring, or its edge, passed to `each` a piece at a time, with
/// whether it is to be reversed.
fn bold_rings(outline: &kurbo::BezPath, style: Style, ring: &mut kurbo::StrokeCtx, what: Ring, mut each: impl FnMut(&[PathEl], bool)) {
    use kurbo::Shape;
    let glyph_sign = outline.area().signum();
    let pen = style.pen();
    for c in subpaths(outline.elements()) {
        kurbo::stroke_with(c.iter().copied(), &pen, &kurbo::StrokeOpts::default(), 1e-3, ring);
        let out = ring.output();
        if what == Ring::Whole {
            // Turned to wind as the glyph does: inside the glyph the two add up, outside the ring alone covers.
            each(out.elements(), out.area().signum() != glyph_sign);
            continue;
        }
        let outer = c.area().signum() == glyph_sign;
        let by_size = |a: &&[PathEl], b: &&[PathEl]| a.area().abs().total_cmp(&b.area().abs());
        let side = if outer { subpaths(out.elements()).max_by(by_size) } else { subpaths(out.elements()).min_by(by_size) };
        if let Some(side) = side {
            each(side, false);
        }
    }
}

/// Appends `els` mapped by `map` to `out`, each subpath reversed when `reversed` (as kurbo's `reverse_subpaths`, in
/// place of a new path).
fn push_mapped(out: &mut kurbo::BezPath, els: &[PathEl], map: Affine, reversed: bool) {
    if !reversed {
        return out.extend(els.iter().map(|&e| map * e));
    }
    let end = |e: &PathEl| match *e {
        PathEl::MoveTo(p) | PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => Some(p),
        PathEl::ClosePath => None,
    };
    for sub in subpaths(els) {
        let closed = matches!(sub.last(), Some(PathEl::ClosePath));
        let body = if closed { &sub[..sub.len() - 1] } else { sub };
        let Some(last) = body.last().and_then(end) else { continue };
        out.move_to(map * last);
        for i in (1..body.len()).rev() {
            let Some(to) = end(&body[i - 1]) else { continue };
            match body[i] {
                PathEl::LineTo(_) => out.line_to(map * to),
                PathEl::QuadTo(c, _) => out.quad_to(map * c, map * to),
                PathEl::CurveTo(c1, c2, _) => out.curve_to(map * c2, map * c1, map * to),
                PathEl::MoveTo(_) | PathEl::ClosePath => {}
            }
        }
        if closed {
            out.close_path();
        }
    }
}

/// Appends a run's glyph outlines to `out`, placed and styled as [`Recorder::glyph_paths`] fills them (skewed for
/// synthetic italic, grown for synthetic bold), in the run's local units: text drawn as a shape's own geometry, filled
/// and stroked as any path. Glyphs without an outline add nothing. `ring`: scratch for synthetic bold, kept between
/// runs.
pub fn glyph_run_outline(fonts: &FontRegistry, run: &GlyphRun<'_>, out: &mut kurbo::BezPath, ring: &mut kurbo::StrokeCtx) {
    if run.glyphs.is_empty() || !(run.size > 0.0 && run.size.is_finite()) {
        return;
    }
    let style = Style { skew_x: run.synthesis.skew_x, bold: if run.synthesis.embolden { embolden_em(run.size as f32) } else { 0.0 } };
    fonts.outlines(run.font, run.glyphs.iter().map(|g| g.0), |k, outline| {
        let map = Affine::translate((run.glyphs[k].1, run.baseline)) * Affine::scale(run.size) * style.skew();
        out.extend(outline.iter().map(|e| map * e));
        if style.bold > 0.0 {
            bold_rings(outline, style, ring, Ring::Whole, |els, reversed| push_mapped(out, els, map, reversed));
        }
    });
}

/// A path's subpaths, each from its `MoveTo`.
fn subpaths(els: &[PathEl]) -> impl Iterator<Item = &[PathEl]> {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start >= els.len() {
            return None;
        }
        let end = els[start + 1..].iter().position(|e| matches!(e, PathEl::MoveTo(_))).map_or(els.len(), |i| start + 1 + i);
        let sub = &els[start..end];
        start = end;
        Some(sub)
    })
}

/// `stroke` made safe to dash in `out`, when it is dashed (returning whether it is): solid when its pattern isn't
/// valid (an entry negative or NaN, the sum 0 or past `f64`) or is too fine for `path`, else its offset within one
/// period (kurbo walks the pattern to an offset entry by entry, so 1e300 never ends). `out`'s pattern keeps its memory.
fn safe_dashes(path: &[PathEl], stroke: &kurbo::Stroke, out: &mut kurbo::Stroke) -> bool {
    let kurbo::Stroke { width, join, miter_limit, start_cap, end_cap, ref dash_pattern, dash_offset } = *stroke;
    if dash_pattern.is_empty() {
        return false;
    }
    (out.width, out.join, out.miter_limit, out.start_cap, out.end_cap) = (width, join, miter_limit, start_cap, end_cap);
    out.dash_pattern.clear();
    let sum: f64 = dash_pattern.iter().sum();
    let period = sum * if dash_pattern.len() % 2 == 1 { 2.0 } else { 1.0 };
    let valid = dash_pattern.iter().all(|&d| d >= 0.0) && sum > 0.0 && period.is_finite();
    // The control polygon is at least as long as the path; NaN or infinity fails the test too.
    if !(valid && polygon_length(path) / period * dash_pattern.len() as f64 <= MAX_DASHES) {
        out.dash_offset = dash_offset;
        return true;
    }
    out.dash_pattern.extend_from_slice(dash_pattern);
    out.dash_offset = dash_offset.rem_euclid(period);
    true
}

/// The length of `path`'s control polygon, closing segments included.
fn polygon_length(path: &[PathEl]) -> f64 {
    let (mut len, mut at, mut start) = (0.0, kurbo::Point::ZERO, kurbo::Point::ZERO);
    for el in path {
        let (points, n) = match *el {
            PathEl::MoveTo(p) => {
                (at, start) = (p, p);
                continue;
            }
            PathEl::LineTo(p) => ([p, p, p], 1),
            PathEl::QuadTo(a, b) => ([a, b, b], 2),
            PathEl::CurveTo(a, b, c) => ([a, b, c], 3),
            PathEl::ClosePath => ([start, start, start], 1),
        };
        for &p in &points[..n] {
            len += at.distance(p);
            at = p;
        }
    }
    len
}

/// Calls `dash(from, to)` with the distances along a straight segment `len` long where each dash of `pattern` (on and
/// off lengths, an odd-length pattern repeated twice, as SVG reads it) starts and ends, in order, starting `offset`
/// into the pattern. `pattern` is what [`safe_dashes`] keeps: non-negative, a finite positive sum, at most
/// [`MAX_DASHES`] entries along the segment.
fn dashes(len: f64, pattern: &[f64], offset: f64, mut dash: impl FnMut(f64, f64)) {
    let n = pattern.len();
    let period = pattern.iter().sum::<f64>() * if n % 2 == 1 { 2.0 } else { 1.0 };
    // The dash `offset` falls in (`i`, counting through the repeated pattern) and what is left of it.
    let (mut i, mut left) = (0, offset.rem_euclid(period));
    while left >= pattern[i % n] {
        left -= pattern[i % n];
        i += 1;
    }
    let (mut pos, mut rest) = (0.0, pattern[i % n] - left);
    while pos < len {
        let end = (pos + rest).min(len);
        if i % 2 == 0 {
            dash(pos, end);
        }
        pos = end;
        i += 1;
        rest = pattern[i % n];
    }
}

/// The rect an axis-aligned segment stroked `width` wide covers, square caps extending each end by half the width.
fn line_rect(a: kurbo::Point, b: kurbo::Point, width: f64, square: bool) -> kurbo::Rect {
    let half = width / 2.0;
    let cap = if square { half } else { 0.0 };
    if a.y == b.y {
        kurbo::Rect::new(a.x.min(b.x) - cap, a.y - half, a.x.max(b.x) + cap, a.y + half)
    } else {
        kurbo::Rect::new(a.x - half, a.y.min(b.y) - cap, a.x + half, a.y.max(b.y) + cap)
    }
}

/// The largest factor `t` stretches a length by: the larger singular value of its 2×2 part.
fn max_scale(t: Affine) -> f64 {
    let [a, b, c, d, _, _] = t.as_coeffs();
    let sum = a * a + b * b + c * c + d * d;
    let det = a * d - b * c;
    ((sum + (sum * sum - 4.0 * det * det).max(0.0).sqrt()) / 2.0).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::Color;
    use kurbo::Shape;

    const RED: Color = Color::rgb(0xFF_0000);

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> kurbo::Rect {
        kurbo::Rect::new(x0, y0, x1, y1)
    }

    fn butt(width: f64) -> kurbo::Stroke {
        kurbo::Stroke::new(width).with_caps(Cap::Butt)
    }

    #[test]
    fn a_dashed_rect_carries_its_pattern_on_from_its_phase() {
        let dashes = |phase: f64| {
            let mut rec = Recorder::new();
            // Four device pixels thick, so a unit of four: dashes of 16 on and 16 off.
            rec.fill_dashed_rect_from(rect(0.0, 0.0, 4.0, 100.0), &[4, 4], phase, RED);
            let mut out = Vec::new();
            let view = Rect { x0: -1.0e6, y0: -1.0e6, x1: 1.0e6, y1: 1.0e6 };
            assert!(rec.lists.dashed[0].each(Transform::from_kurbo(kurbo::Affine::IDENTITY), view, |r| out.push((r.y0, r.y1))));
            out
        };
        assert_eq!(dashes(0.0)[0], (0.0, 16.0));
        // Starting 16 into the pattern, the rect opens in the gap: its first dash is 16 further on.
        assert_eq!(dashes(16.0)[0], (16.0, 32.0));
        // Starting 8 into a dash, the rest of it is drawn.
        assert_eq!(dashes(8.0)[0], (0.0, 8.0));
    }

    #[test]
    fn commands_stay_small() {
        assert!(std::mem::size_of::<Cmd>() <= 24, "{}", std::mem::size_of::<Cmd>());
    }

    #[test]
    fn pictures_are_send_sync_and_unique() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Picture>();
        let mut rec = Recorder::new();
        let (a, b) = (rec.finish(), rec.finish());
        assert_ne!(a.id(), b.id());
        assert_eq!(a.bounds(), None);
    }

    #[test]
    fn solid_rects_record_premultiplied_and_inline() {
        let mut rec = Recorder::new();
        rec.fill_rect(rect(4.0, 3.0, 1.0, 1.0), Paint { aa: false, ..RED.with_alpha(128).into() });
        let pic = rec.finish();
        let want = Cmd::SolidRect {
            rect: Rect { x0: 1.0, y0: 1.0, x1: 4.0, y1: 3.0 },
            color: PremulColor { r: 128, g: 0, b: 0, a: 128 },
            aa: false,
            blend: BlendMode::SrcOver,
        };
        assert_eq!(pic.lists().cmds, [want]);
        assert_eq!(pic.bounds(), Some(rect(1.0, 1.0, 4.0, 3.0)));
    }

    #[test]
    fn rect_paths_and_unrounded_rrects_become_rects() {
        let mut rec = Recorder::new();
        rec.fill_path(rect(0.0, 0.0, 2.0, 2.0).to_path(0.1).elements(), RED);
        rec.fill_rrect(rect(0.0, 0.0, 2.0, 2.0), 0.0, RED);
        rec.fill_rrect(rect(0.0, 0.0, 2.0, 2.0), -3.0, RED);
        let pic = rec.finish();
        assert!(pic.lists().cmds.iter().all(|c| matches!(c, Cmd::SolidRect { .. })), "{:?}", pic.lists().cmds);
        assert!(pic.lists().verbs.is_empty() && pic.lists().rrects.is_empty());
    }

    #[test]
    fn radii_that_do_not_fit_scale_together() {
        assert_eq!(fit_radii(rect(0.0, 0.0, 10.0, 4.0), RoundedRectRadii::from_single_radius(3.0)), [2.0; 4]);
        assert_eq!(fit_radii(rect(0.0, 0.0, 10.0, 10.0), RoundedRectRadii::new(1.0, 2.0, 3.0, 4.0)), [1.0, 2.0, 3.0, 4.0]);
        let mut rec = Recorder::new();
        rec.fill_rrect(rect(0.0, 0.0, 10.0, 4.0), 3.0, RED);
        let pic = rec.finish();
        assert!(matches!(pic.lists().cmds[..], [Cmd::RRect { rrect: 0, paint: 0 }]));
        assert_eq!(pic.lists().rrects[0].1, [2.0; 4]);
    }

    #[test]
    fn axis_aligned_strokes_become_rects() {
        let mut rec = Recorder::new();
        rec.line((0.0, 5.0).into(), (10.0, 5.0).into(), &butt(2.0), RED);
        rec.line((3.0, 0.0).into(), (3.0, 8.0).into(), &kurbo::Stroke::new(1.0).with_caps(Cap::Square), RED);
        let pic = rec.finish();
        let rects: Vec<_> =
            pic.lists().cmds.iter().map(|c| if let Cmd::SolidRect { rect, .. } = c { rect.to_kurbo() } else { panic!("{c:?}") }).collect();
        assert_eq!(rects, [rect(0.0, 4.0, 10.0, 6.0), rect(2.5, -0.5, 3.5, 8.5)]);
    }

    #[test]
    fn mitered_rect_outlines_become_frames() {
        let mut rec = Recorder::new();
        let pen = |w: f64| butt(w).with_join(Join::Miter);
        rec.stroke_path(rect(0.0, 0.0, 10.0, 6.0).to_path(0.1).elements(), &pen(1.0), RED);
        // Too wide for a hole: the outer rect.
        rec.stroke_path(rect(0.0, 0.0, 10.0, 6.0).to_path(0.1).elements(), &pen(6.0), RED);
        // Round joins, a miter limit under √2, dashes and an open rect stay paths.
        rec.stroke_path(rect(0.0, 0.0, 10.0, 6.0).to_path(0.1).elements(), &butt(1.0).with_join(Join::Round), RED);
        rec.stroke_path(rect(0.0, 0.0, 10.0, 6.0).to_path(0.1).elements(), &pen(1.0).with_miter_limit(1.4), RED);
        rec.stroke_path(rect(0.0, 0.0, 10.0, 6.0).to_path(0.1).elements(), &pen(1.0).with_dashes(0.0, [2.0, 1.0]), RED);
        let open = rect(0.0, 0.0, 10.0, 6.0).to_path(0.1).elements().iter().copied().filter(|e| *e != PathEl::ClosePath).collect::<Vec<_>>();
        rec.stroke_path(&open, &pen(1.0), RED);
        let pic = rec.finish();
        let l = pic.lists();
        assert!(matches!(l.cmds[0], Cmd::Frame { frame: 0, .. }));
        assert_eq!(l.frames, [(Rect { x0: -0.5, y0: -0.5, x1: 10.5, y1: 6.5 }, Rect { x0: 0.5, y0: 0.5, x1: 9.5, y1: 5.5 })]);
        assert!(matches!(l.cmds[1], Cmd::SolidRect { rect: Rect { x0: -3.0, y0: -3.0, x1: 13.0, y1: 9.0 }, .. }));
        assert!(l.cmds[2..].iter().all(|c| matches!(c, Cmd::Path { .. })), "{:?}", &l.cmds[2..]);
        assert_eq!(pic.bounds(), Some(rect(-3.0, -3.0, 13.0, 9.0)));
    }

    #[test]
    fn dashed_axis_aligned_strokes_become_one_rect_per_dash() {
        let mut rec = Recorder::new();
        rec.line((0.0, 0.0).into(), (10.0, 0.0).into(), &butt(1.0).with_dashes(0.0, [3.0, 1.0]), RED);
        let pic = rec.finish();
        let xs: Vec<_> =
            pic.lists().cmds.iter().map(|c| if let Cmd::SolidRect { rect, .. } = c { (rect.x0, rect.x1) } else { panic!("{c:?}") }).collect();
        assert_eq!(xs, [(0.0, 3.0), (4.0, 7.0), (8.0, 10.0)]);
    }

    #[test]
    fn filter_sizes_are_capped_and_nan_is_none() {
        assert_eq!(Filter::Blur { sigma: -3.0 }.device_size(2.0), 6.0);
        assert_eq!(Filter::Blur { sigma: 1.0e9 }.device_size(1.0), MAX_FILTER_PX);
        assert_eq!(Filter::Erode { radius: f32::INFINITY }.device_size(1.0), MAX_FILTER_PX);
        assert_eq!(Filter::Erode { radius: f32::NAN }.device_size(1.0), 0.0);
    }

    #[test]
    fn dashes_follow_offset_and_repeat_odd_patterns() {
        let collect = |len: f64, pattern: &[f64], offset: f64| {
            let mut out = Vec::new();
            dashes(len, pattern, offset, |a, b| out.push((a, b)));
            out
        };
        assert_eq!(collect(10.0, &[3.0, 1.0], 2.0), [(0.0, 1.0), (2.0, 5.0), (6.0, 9.0)]);
        assert_eq!(collect(10.0, &[3.0, 1.0], -1.0), [(1.0, 4.0), (5.0, 8.0), (9.0, 10.0)]);
        assert_eq!(collect(6.0, &[1.0], 0.0), [(0.0, 1.0), (2.0, 3.0), (4.0, 5.0)]);
        // Too fine (`safe_dashes`): solid.
        let mut rec = Recorder::new();
        rec.line((0.0, 0.0).into(), (1e9, 0.0).into(), &butt(1.0).with_dashes(0.0, [1.0, 1.0]), RED);
        assert_eq!(rec.finish().lists().cmds.len(), 1);
    }

    #[test]
    fn round_caps_diagonals_and_curves_become_paths() {
        let mut rec = Recorder::new();
        rec.line((0.0, 0.0).into(), (10.0, 0.0).into(), &kurbo::Stroke::new(2.0), RED);
        rec.line((0.0, 0.0).into(), (10.0, 10.0).into(), &butt(2.0), RED);
        rec.stroke_path(kurbo::Circle::new((0.0, 0.0), 5.0).to_path(0.1).elements(), &butt(1.0), RED);
        let pic = rec.finish();
        assert_eq!(pic.lists().cmds.len(), 3);
        assert!(pic.lists().cmds.iter().all(|c| matches!(c, Cmd::Path { .. })), "{:?}", pic.lists().cmds);
        // Round caps reach a half width past the ends.
        let b = pic.bounds().expect("drawn");
        assert!(b.x0 <= -1.0 + 1e-6 && b.x1 >= 11.0 - 1e-6, "{b:?}");
    }

    #[test]
    fn zero_width_strokes_draw_nothing_and_bad_dashes_are_solid() {
        let mut rec = Recorder::new();
        rec.line((0.0, 0.0).into(), (10.0, 0.0).into(), &butt(0.0), RED);
        rec.line((0.0, 0.0).into(), (10.0, 0.0).into(), &butt(1.0).with_dashes(0.0, [0.0, 0.0]), RED);
        let pic = rec.finish();
        let rects: Vec<_> =
            pic.lists().cmds.iter().map(|c| if let Cmd::SolidRect { rect, .. } = c { rect.to_kurbo() } else { panic!("{c:?}") }).collect();
        assert_eq!(rects, [rect(0.0, -0.5, 10.0, 0.5)]);
    }

    #[test]
    fn bounds_follow_transforms_clips_and_layers() {
        let mut rec = Recorder::new();
        rec.push_transform(Affine::translate((100.0, 0.0)));
        rec.push_clip(Clip::Rect { rect: rect(0.0, 0.0, 5.0, 5.0), aa: true });
        rec.fill_rect(rect(-10.0, -10.0, 3.0, 3.0), RED);
        rec.pop();
        rec.push_layer(&Layer { filters: &[Filter::Erode { radius: 1.0 }, Filter::Blur { sigma: 2.0 }], ..Layer::default() });
        rec.push_transform(Affine::scale(2.0));
        rec.fill_rect(rect(10.0, 10.0, 11.0, 11.0), RED);
        let pic = rec.finish();
        // The clipped rect: 100..103 × 0..3. The blurred one: 120..122 × 20..22, grown by 3 sigma = 6.
        assert_eq!(pic.bounds(), Some(rect(100.0, 0.0, 128.0, 28.0)));
        let pops = pic.lists().cmds.iter().filter(|c| matches!(c, Cmd::Pop)).count();
        let pushes =
            pic.lists().cmds.iter().filter(|c| matches!(c, Cmd::PushTransform { .. } | Cmd::PushClip { .. } | Cmd::PushLayer { .. })).count();
        assert_eq!((pushes, pops), (4, 4), "finish ends the pushes left open");
    }

    #[test]
    fn layer_bounds_clip_and_scale_its_blur() {
        let mut rec = Recorder::new();
        rec.push_transform(Affine::scale(2.0));
        rec.push_layer(&Layer { filters: &[Filter::Blur { sigma: 1.0 }], bounds: Some(rect(0.0, 0.0, 4.0, 4.0)), ..Layer::default() });
        rec.fill_rect(rect(-50.0, -50.0, 50.0, 50.0), RED);
        let pic = rec.finish();
        // Clipped to the layer: 0..8 in the picture, then grown by 3 sigma scaled by 2.
        assert_eq!(pic.bounds(), Some(rect(-6.0, -6.0, 14.0, 14.0)));
    }

    #[test]
    fn nested_pictures_are_shared_and_placed() {
        let mut rec = Recorder::new();
        rec.fill_rect(rect(0.0, 0.0, 10.0, 2.0), RED);
        let line = rec.finish();
        rec.picture(&line, Affine::translate((5.0, 40.0)));
        rec.picture(&line, Affine::translate((5.0, 60.0)));
        let lines = rec.finish();
        let empty = rec.finish();
        rec.picture(&lines, Affine::IDENTITY);
        rec.picture(&empty, Affine::IDENTITY);
        let page = rec.finish();
        assert_eq!(page.bounds(), Some(rect(5.0, 40.0, 15.0, 62.0)));
        assert_eq!(page.lists().children.len(), 1, "an empty picture is not placed");
        let inner = &page.lists().children[0];
        assert!(Arc::ptr_eq(&inner.lists().children[0].0, &line.0) && Arc::ptr_eq(&inner.lists().children[1].0, &line.0));
    }

    #[test]
    fn every_command_records_its_payload() {
        let stops = [Stop { offset: 0.0, color: RED }, Stop { offset: 1.0, color: Color::WHITE }];
        let filters = [ColorFilter::Table([7; 256]), ColorFilter::Matrix([0.5; 20])];
        let mut rec = Recorder::new();
        let gradient =
            Brush::Linear { start: (0.0, 0.0).into(), end: (1.0, 0.0).into(), stops: &stops, extend: Extend::Mirror, transform: Affine::IDENTITY };
        rec.fill_rect(rect(0.0, 0.0, 1.0, 1.0), Paint::new(gradient));
        let font = FontId(3);
        let glyphs = [(36, 10.0), (72, 16.5)];
        let synthesis = Synthesis { embolden: true, skew_x: -0.2 };
        rec.glyphs(&FontRegistry::default(), &GlyphRun { font, size: 12.0, baseline: 100.0, glyphs: &glyphs, synthesis }, Color::BLACK);
        rec.image(
            ImageId(9),
            Some(rect(0.0, 0.0, 8.0, 8.0)),
            rect(0.0, 0.0, 16.0, 16.0),
            &ImagePaint { sampling: Sampling::Nearest, color_filters: &filters },
        );
        rec.push_clip(Clip::Path(kurbo::Circle::new((0.0, 0.0), 4.0).to_path(0.1).elements()));
        rec.push_layer(&Layer { opacity: 3.0, blend: BlendMode::DstIn, color_filters: &filters[..1], ..Layer::default() });
        let pic = rec.finish();
        let l = pic.lists();
        assert!(matches!(l.paints[0].brush, BrushRec::Linear { stops: Span { start: 0, len: 2 }, extend: Extend::Mirror, .. }));
        assert_eq!(l.stops, stops);
        let none = Span::default();
        assert_eq!(
            l.runs[0],
            RunRec { font, size: 12.0, baseline: 100.0, glyphs: Span { start: 0, len: 2 }, synthesis, text: none, clusters: none, invisible: false }
        );
        assert_eq!(l.glyphs, [GlyphRec { raw: 36, x: 10.0 }, GlyphRec { raw: 72, x: 16.5 }]);
        assert_eq!(l.images[0].color_filters, Span { start: 0, len: 2 }, "filters are kept as a chain, in order");
        assert_eq!(l.color_filters[..2], filters);
        assert!(matches!(l.clips[0], ClipRec::Path { path: PathSpan { verbs: Span { len: 6.., .. }, .. }, .. }));
        assert_eq!((l.layers[0].opacity, l.layers[0].blend, l.layers[0].color_filters), (1.0, BlendMode::DstIn, Span { start: 2, len: 1 }));
    }

    #[test]
    fn glyph_runs_are_bounded_by_their_ink() {
        let mut fonts = FontRegistry::default();
        let mono: crate::font::FontData = Arc::new(include_bytes!("../tests/fonts/jetbrains-mono-500.ttf").to_vec());
        let font = fonts.add(mono.clone(), 0, &[]).expect("the test face registers");
        let id = |c| skrifa::MetadataProvider::charmap(&skrifa::FontRef::new((*mono).as_ref()).expect("reads")).map(c).expect("has it").to_u32();
        let glyphs = [(id('x'), 10.0), (id('j'), 20.0)];
        let mut rec = Recorder::new();
        rec.glyphs(&fonts, &GlyphRun { font, size: 20.0, baseline: 50.0, glyphs: &glyphs, synthesis: Synthesis::default() }, Color::BLACK);
        let b = rec.finish().bounds().expect("ink");
        // An x sits on the baseline; a j has a dot above x-height and a descender. Neither reaches far past its cell.
        assert!(b.x0 > 9.0 && b.x1 < 32.0, "{b:?}");
        assert!(b.y1 > 52.0 && b.y1 < 58.0 && b.y0 > 30.0 && b.y0 < 42.0, "{b:?}");
        let mut rec = Recorder::new();
        let italic = Synthesis { embolden: false, skew_x: -0.2 };
        rec.glyphs(&fonts, &GlyphRun { font, size: 20.0, baseline: 50.0, glyphs: &glyphs, synthesis: italic }, Color::BLACK);
        let slanted = rec.finish().bounds().expect("ink");
        // Tops lean right; the x's foot is on the baseline, where the skew moves nothing; heights don't change.
        assert!(slanted.x1 > b.x1 + 1.0 && slanted.x0 <= b.x0 && (slanted.y0 - b.y0).abs() < 1e-9, "{slanted:?} vs {b:?}");
        let mut rec = Recorder::new();
        let bold = Synthesis { embolden: true, skew_x: 0.0 };
        rec.glyphs(&fonts, &GlyphRun { font, size: 20.0, baseline: 50.0, glyphs: &glyphs, synthesis: bold }, Color::BLACK);
        let heavy = rec.finish().bounds().expect("ink");
        // Half the pen's width, times its miter limit for the sharp corners.
        let grow = 20.0 * embolden_em(20.0) as f64 / 2.0 * 4.0;
        // The bold width is an f32; the box is f64.
        assert!((heavy.x0 - (b.x0 - grow)).abs() < 1e-4 && (heavy.y1 - (b.y1 + grow)).abs() < 1e-4, "{heavy:?} vs {b:?}");
    }

    #[test]
    fn a_run_outline_is_the_path_glyph_paths_fill() {
        use crate::cpu::{Draw, Format, Frame, Renderer, Target};
        let mut resources = crate::Resources::default();
        let mono: crate::font::FontData = Arc::new(include_bytes!("../tests/fonts/jetbrains-mono-500.ttf").to_vec());
        let font = resources.fonts.add(mono, 0, &[]).expect("the test face registers");
        let fonts = &resources.fonts;
        let glyphs: Vec<(u32, f64)> = (0..6).map(|i| (i + 30, i as f64 * 12.0)).collect();
        // Plain, each glyph's em outline placed at its origin and size, in order.
        let run = GlyphRun { font, size: 20.0, baseline: 50.0, glyphs: &glyphs, synthesis: Synthesis::default() };
        let mut outline = kurbo::BezPath::new();
        glyph_run_outline(fonts, &run, &mut outline, &mut kurbo::StrokeCtx::default());
        let mut want = kurbo::BezPath::new();
        for &(g, x) in &glyphs {
            let mut em = kurbo::BezPath::new();
            assert!(fonts.em_outline(font, g, &mut em));
            want.extend(em.iter().map(|e| Affine::translate((x, 50.0)) * Affine::scale(20.0) * e));
        }
        assert_eq!(outline, want);
        // Synthetic bold and italic: the path covers the pixels glyph_paths does.
        let draw = |picture: &Picture| {
            let mut px = vec![0u8; 96 * 80 * 4];
            let draws = [Draw { picture, transform: Affine::IDENTITY }];
            let frame = Frame { clear: Some(Color::WHITE), draws: &draws };
            Renderer::new().render(&mut Target::new(&mut px, 96, 80, 96 * 4, Format::Rgba8).expect("fits"), &resources, &frame);
            px
        };
        let run = GlyphRun { synthesis: Synthesis { embolden: true, skew_x: -0.2 }, ..run };
        let mut rec = Recorder::new();
        rec.glyph_paths(fonts, &run, Color::BLACK);
        let painted = rec.finish();
        let mut outline = kurbo::BezPath::new();
        glyph_run_outline(fonts, &run, &mut outline, &mut kurbo::StrokeCtx::default());
        let mut rec = Recorder::new();
        rec.fill_path(outline.elements(), Color::BLACK);
        let filled = rec.finish();
        assert!(outline.elements().len() > want.elements().len(), "the bold rings are added");
        assert_eq!(filled.bounds(), painted.bounds());
        assert!(draw(&filled) == draw(&painted), "the same pixels");
        let mut none = kurbo::BezPath::new();
        glyph_run_outline(
            fonts,
            &GlyphRun { font, size: f64::NAN, baseline: 0.0, glyphs: &glyphs, synthesis: Synthesis::default() },
            &mut none,
            &mut kurbo::StrokeCtx::default(),
        );
        assert!(none.is_empty());
    }

    #[test]
    fn a_reused_recorder_records_without_allocating() {
        let mut rec = Recorder::new();
        let mut fonts = FontRegistry::default();
        let mono: crate::font::FontData = Arc::new(include_bytes!("../tests/fonts/jetbrains-mono-500.ttf").to_vec());
        let font = fonts.add(mono, 0, &[]).expect("the test face registers");
        let glyphs: Vec<(u32, f64)> = (0..60).map(|i| (i + 3, i as f64 * 6.0)).collect();
        let curve = [PathEl::MoveTo((0.0, 0.0).into()), PathEl::CurveTo((5.0, -5.0).into(), (10.0, 5.0).into(), (15.0, 0.0).into())];
        let page = |rec: &mut Recorder| {
            for line in 0..40 {
                let y = line as f64 * 14.0;
                rec.fill_rect(rect(0.0, y, 400.0, y + 1.0), RED);
                let run = GlyphRun { font, size: 11.0, baseline: y + 11.0, glyphs: &glyphs, synthesis: Synthesis::default() };
                rec.glyphs(&fonts, &run, Color::BLACK);
                rec.stroke_glyphs(&fonts, &run, &kurbo::Stroke::new(0.5), RED);
                let bold = GlyphRun { synthesis: Synthesis { embolden: true, skew_x: -0.2 }, ..run };
                rec.stroke_glyphs(&fonts, &bold, &kurbo::Stroke::new(0.5), RED);
                rec.glyph_paths(&fonts, &bold, RED);
                rec.stroke_path(&curve, &kurbo::Stroke::new(1.0), RED);
                rec.push_clip(Clip::Rect { rect: rect(0.0, y, 400.0, y + 14.0), aa: true });
                rec.line((0.0, y).into(), (400.0, y).into(), &butt(0.5).with_dashes(0.0, [2.0, 2.0]), RED);
                rec.pop();
            }
        };
        page(&mut rec);
        let first = rec.finish();
        let recording = allocation_counter::measure(|| page(&mut rec));
        assert_eq!(recording.count_total, 0, "recording into a reused recorder");
        let finishing = allocation_counter::measure(|| drop(rec.finish()));
        // The page's non-empty lists: cmds, paints, verbs, points, runs, glyphs and clips.
        let lists = 7;
        assert!(finishing.count_total <= lists + 1, "one allocation per non-empty list and one for the picture: {}", finishing.count_total);
        assert!(first.bytes() > 40 * 60 * std::mem::size_of::<GlyphRec>());
        // Dashed glyph outlines, with patterns past the four entries a stroke keeps inline and within them.
        let run = GlyphRun { font, size: 11.0, baseline: 11.0, glyphs: &glyphs, synthesis: Synthesis::default() };
        let long = kurbo::Stroke::new(0.5).with_dashes(3.0, [1.0, 0.5, 2.0, 0.5, 0.25, 0.5]);
        let short = kurbo::Stroke::new(0.5).with_dashes(3.0, [1.0, 0.5, 2.0, 0.5]);
        rec.stroke_glyphs(&fonts, &run, &long, RED);
        rec.stroke_glyphs(&fonts, &run, &short, RED);
        let dashing = allocation_counter::measure(|| {
            rec.stroke_glyphs(&fonts, &run, &long, RED);
            rec.stroke_glyphs(&fonts, &run, &short, RED);
        });
        assert_eq!(dashing.count_total, 0, "dashing glyph outlines into a warm recorder");
        // A bold run's outline, its scratch kept.
        let bold = GlyphRun { font, size: 11.0, baseline: 11.0, glyphs: &glyphs, synthesis: Synthesis { embolden: true, skew_x: 0.0 } };
        let (mut out, mut ring) = (kurbo::BezPath::new(), kurbo::StrokeCtx::default());
        glyph_run_outline(&fonts, &bold, &mut out, &mut ring);
        let outlining = allocation_counter::measure(|| {
            out.truncate(0);
            glyph_run_outline(&fonts, &bold, &mut out, &mut ring);
        });
        assert_eq!(outlining.count_total, 0, "outlining a bold run with kept scratch");
    }
}
