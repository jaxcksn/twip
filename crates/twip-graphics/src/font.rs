//! Faces and glyph masks: the face registry (font bytes shared with Twip, a face index and variation coordinates), and
//! the cache of glyph coverage masks the backends blit text from.
//!
//! A glyph starts as its outline in em space (from skrifa, unhinted, y down), with synthetic italic's skew and
//! synthetic bold's outset applied there, in the text's own space, as Skia applies them. A glyph mask is that outline
//! mapped by the text's device matrix (em to pixels: size, zoom, rotation, skew, flips), offset to its subpixel
//! position and rasterised once into 8-bit coverage, then reused by every draw with the same key. Like Skia, text
//! whose em is more than 256 pixels across is drawn from its outline as a path instead of a mask.
//!
//! - Hinting: text at a uniform scale and without skew, upright or turned by quarter turns (vertical sections, turned
//!   cells), is hinted lightly (skrifa's light smooth target with linear metrics: glyph origins and advances kept,
//!   edges snapped across the baseline, and along it moved by up to half a pixel where a font's instructions move
//!   them), as both of Twip's Skia scalers hint it (DirectWrite natively, FreeType on the web). The hinted outline is the upright one, in pixels, then turned: a quarter turn keeps the pixel grid. Against
//!   DirectWrite, hinting cut the share of text pixels that differ by more than 64 from 14% to 4% at dpr 1 and from 16%
//!   to 0.5% at dpr 2. Other transforms (any other angle, mirrored, skewed) are unhinted, as hinting assumes the grid.
//! - Subpixel positions: a quarter pixel along the baseline for upright and quarter-turned text, hinted or not (skewed
//!   or stretched too: the baseline's own coordinate is rounded to a whole pixel), a quarter pixel both ways for any
//!   other transform, as Skia's strikes do.
//! - Synthetic bold strokes each contour by Skia's fake-bold width (`size × lerp(1/24 at 9, 1/32 at 36)`, in em units)
//!   and adds the stroke to the fill, each stroke turned to wind the same way as the glyph, so the non-zero fill of
//!   the two together is the outset glyph (Skia's stroke-and-fill).
//! - Masks go through Skia's A8 mask gamma (`SkMaskGamma`, contrast 0.5, sRGB luminance; Twip's Skia builds apply it
//!   to A8 masks): a lookup from coverage to coverage, chosen by the text colour's luminance (8 levels). Glyphs drawn
//!   as paths go through the same lookup (Skia gives them none), so text keeps its weight across 256 pixels per em.
//!
//! Shaping is not here: Twip shapes with harfrust and records glyph ids and positions.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};

use kurbo::{BezPath, PathEl, Shape};
use skrifa::bitmap::BitmapStrikes;
use skrifa::instance::{Location, Size};
use skrifa::outline::{DrawSettings, HintingInstance, OutlinePen, SmoothMode, Target};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider, Tag};

use crate::fx::FastMap;
use crate::geom::Point;
use crate::path::Verb;
use crate::raster::{IRect, Rasterizer};

/// A face in the registry. Pictures name faces by id, never by pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(pub(crate) u32);

/// Font file bytes, shared with the caller (Twip's faces hold an `Arc<Vec<u8>>`, which coerces to this).
pub type FontData = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// Bytes that aren't a readable font, or a face index the file doesn't have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontError {
    /// The face index asked for.
    pub index: u32,
    /// What was wrong with the bytes.
    pub message: String,
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "face {} doesn't read: {}", self.index, self.message)
    }
}

impl std::error::Error for FontError {}

struct Face {
    data: FontData,
    index: u32,
    location: Location,
    /// The variation coordinates it was registered with (axis tag, user value).
    #[cfg_attr(not(feature = "pdf"), allow(dead_code, reason = "the PDF writer embeds the face at them"))]
    variations: Vec<([u8; 4], f32)>,
}

/// A variation coordinate: an axis tag and its user value.
#[cfg(feature = "pdf")]
pub(crate) type Variation = ([u8; 4], f32);

/// A face's font bytes' address, index and variations (axis tags and values as `f32` bits). The face holds its bytes,
/// so the address isn't reused while the key is in the map.
type FaceKey = (usize, u32, Vec<([u8; 4], u32)>);

/// The faces pictures draw with.
pub struct FontRegistry {
    /// This registry's own number: its ids are its own.
    pub(crate) registry: u64,
    faces: Vec<Face>,
    /// Faces by what they were registered with, so registering a face twice gives the same id.
    known: HashMap<FaceKey, FontId>,
    /// Each glyph's control box in em units, y down (`None` for a glyph without ink), filled as recording asks.
    /// Behind a lock because recording borrows the registry shared, and the registry may be shared across threads.
    /// Keyed by `font << 32 | glyph` (one hasher step; recording looks up every glyph).
    boxes: Mutex<FastMap<u64, Option<kurbo::Rect>>>,
    /// Each glyph's outline in em units, y down, as stroked text reads it (`None` for a glyph without one, or a colour
    /// or bitmap glyph), filled as recording asks and keyed as `boxes` is: at most one per face and glyph id.
    outlines: Mutex<FastMap<u64, Option<BezPath>>>,
}

impl fmt::Debug for FontRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontRegistry").field("faces", &self.faces.len()).finish()
    }
}

impl Default for FontRegistry {
    fn default() -> Self {
        FontRegistry {
            registry: crate::next_registry(),
            faces: Vec::new(),
            known: HashMap::new(),
            boxes: Mutex::default(),
            outlines: Mutex::default(),
        }
    }
}

impl FontRegistry {
    /// Registers face `index` of `data` (TrueType, OpenType or a collection; Twip decodes WOFF first) at the variation
    /// coordinates `variations` (axis tag, user value). The same bytes, index and coordinates give the same id.
    pub fn add(&mut self, data: FontData, index: u32, variations: &[([u8; 4], f32)]) -> Result<FontId, FontError> {
        let address = Arc::as_ptr(&data) as *const () as usize;
        let key = (address, index, variations.iter().map(|&(t, v)| (t, v.to_bits())).collect::<Vec<_>>());
        if let Some(&id) = self.known.get(&key) {
            return Ok(id);
        }
        let font = FontRef::from_index((*data).as_ref(), index).map_err(|e| FontError { index, message: e.to_string() })?;
        let location = font.axes().location(variations.iter().map(|&(t, v)| (Tag::new(&t), v)));
        let id = FontId(self.faces.len() as u32);
        self.faces.push(Face { data, index, location, variations: variations.to_vec() });
        self.known.insert(key, id);
        Ok(id)
    }

    /// The face's bytes, index and variation coordinates, as registered.
    #[cfg(feature = "pdf")]
    pub(crate) fn source(&self, font: FontId) -> Option<(&FontData, u32, &[Variation])> {
        self.faces.get(font.0 as usize).map(|f| (&f.data, f.index, &f.variations[..]))
    }

    /// The face's font, and its variation location.
    pub(crate) fn font_ref(&self, font: FontId) -> Option<(FontRef<'_>, &Location)> {
        let face = self.faces.get(font.0 as usize)?;
        Some((FontRef::from_index((*face.data).as_ref(), face.index).ok()?, &face.location))
    }

    /// What draws `glyph`: a COLR colour glyph, then a colour bitmap (CBDT, sbix), then its outline. Monochrome and
    /// grey embedded bitmaps (EBDT, which many system fonts carry for small sizes) are not used: Skia draws them only
    /// when asked (`SkFont::setEmbeddedBitmaps`, off by default), and Twip never asks.
    pub(crate) fn kind(&self, font: FontId, glyph: u32) -> GlyphKind {
        let Some((f, _)) = self.font_ref(font) else { return GlyphKind::Outline };
        let id = GlyphId::new(glyph);
        if f.color_glyphs().get(id).is_some() {
            GlyphKind::Colr
        } else if colour_strikes(&f).is_some_and(|s| s.glyph_for_size(Size::unscaled(), id).is_some()) {
            GlyphKind::Bitmap
        } else {
            GlyphKind::Outline
        }
    }

    /// A colour glyph's box in em units, y down: a COLRv1 clip box, else the boxes of what it clips to, else a bitmap's
    /// placement.
    fn colour_box(&self, font: FontId, glyph: u32) -> Option<kurbo::Rect> {
        let (f, location) = self.font_ref(font)?;
        let upem = f.head().ok()?.units_per_em() as f64;
        let id = GlyphId::new(glyph);
        let em = |b: kurbo::Rect| kurbo::Rect::new(b.x0 / upem, -b.y1 / upem, b.x1 / upem, -b.y0 / upem);
        if let Some(g) = f.color_glyphs().get(id) {
            if let Some(b) = g.bounding_box(location, Size::unscaled()) {
                return Some(em(kurbo::Rect::new(b.x_min as f64, b.y_min as f64, b.x_max as f64, b.y_max as f64)));
            }
            let mut painter = BoundsPainter { fonts: self, font, upem, transforms: vec![kurbo::Affine::IDENTITY], clips: Vec::new(), ink: None };
            g.paint(location, &mut painter).ok()?;
            return painter.ink.map(em);
        }
        let b = colour_strikes(&f)?.glyph_for_size(Size::unscaled(), id)?;
        Some(bitmap_em_box(&b, upem))
    }

    /// Appends glyph `glyph`'s outline in em units (1 em = 1.0), y down, to `out`. False when the face or glyph is
    /// missing or its outline doesn't read.
    pub fn em_outline(&self, font: FontId, glyph: u32, out: &mut BezPath) -> bool {
        let Some(face) = self.faces.get(font.0 as usize) else { return false };
        let Ok(f) = FontRef::from_index((*face.data).as_ref(), face.index) else { return false };
        let Some(g) = f.outline_glyphs().get(GlyphId::new(glyph)) else { return false };
        // In font units, scaled here: skrifa scales TrueType outlines in 26.6 fixed point, which at one pixel per em
        // rounds every point to 1/64 em (a visible notch at display sizes).
        let Some(upem) = f.head().ok().map(|h| h.units_per_em()).filter(|&u| u > 0) else { return false };
        let start = out.elements().len();
        if g.draw(DrawSettings::unhinted(Size::unscaled(), &face.location), &mut Pen(out)).is_err() {
            return false;
        }
        let em = 1.0 / f64::from(upem);
        for el in &mut out.elements_mut()[start..] {
            *el = kurbo::Affine::scale(em) * *el;
        }
        true
    }

    /// A light (vertical-only) hinting instance for `font` at `ppem` pixels per em, which keeps advances and
    /// horizontal positions: `None` when the face has none to give.
    fn hinting(&self, font: FontId, ppem: f32) -> Option<HintingInstance> {
        let face = self.faces.get(font.0 as usize)?;
        let f = FontRef::from_index((*face.data).as_ref(), face.index).ok()?;
        let target = Target::Smooth { mode: SmoothMode::Light, symmetric_rendering: true, preserve_linear_metrics: true };
        HintingInstance::new(&f.outline_glyphs(), Size::new(ppem), &face.location, target).ok()
    }

    /// Appends glyph `glyph`'s outline hinted by `hinting`, in pixels, y down, to `out`.
    fn hinted_outline(&self, font: FontId, glyph: u32, hinting: &HintingInstance, out: &mut BezPath) -> bool {
        let Some(face) = self.faces.get(font.0 as usize) else { return false };
        let Ok(f) = FontRef::from_index((*face.data).as_ref(), face.index) else { return false };
        let Some(g) = f.outline_glyphs().get(GlyphId::new(glyph)) else { return false };
        g.draw(DrawSettings::hinted(hinting, false), &mut Pen(out)).is_ok()
    }

    /// Calls `each(k, outline)` for the `k`th of `glyphs` that has an outline, with that outline in em units, y down,
    /// reading the outlines it hasn't kept.
    pub(crate) fn outlines(&self, font: FontId, glyphs: impl Iterator<Item = u32>, mut each: impl FnMut(usize, &BezPath)) {
        // A poisoned lock still holds correct outlines: each is read before it's inserted.
        let mut outlines = self.outlines.lock().unwrap_or_else(|e| e.into_inner());
        for (k, glyph) in glyphs.enumerate() {
            let glyph = glyph_id(glyph);
            let outline = outlines.entry(u64::from(font.0) << 32 | u64::from(glyph)).or_insert_with(|| {
                let mut path = BezPath::new();
                (self.kind(font, glyph) == GlyphKind::Outline && self.em_outline(font, glyph, &mut path) && !path.is_empty()).then_some(path)
            });
            if let Some(outline) = outline {
                each(k, outline);
            }
        }
    }

    /// Calls `each(glyph, box)` for each of `glyphs` with its control box in em units, y down (`None` without ink),
    /// computing the boxes it hasn't seen.
    pub(crate) fn boxes(&self, font: FontId, glyphs: impl Iterator<Item = u32>, mut each: impl FnMut(u32, Option<kurbo::Rect>)) {
        // A poisoned lock still holds correct boxes: they are computed before they're inserted.
        let mut boxes = self.boxes.lock().unwrap_or_else(|e| e.into_inner());
        let mut outline = BezPath::new();
        // Raw ids (not `glyph_id`'s): the clamp cost recording 4%, and a face's ids are its own, from the host, never
        // a document's.
        for glyph in glyphs {
            let b = *boxes.entry(u64::from(font.0) << 32 | u64::from(glyph)).or_insert_with(|| match self.kind(font, glyph) {
                GlyphKind::Outline => {
                    outline.truncate(0);
                    (self.em_outline(font, glyph, &mut outline) && !outline.is_empty()).then(|| outline.control_box())
                }
                GlyphKind::Colr | GlyphKind::Bitmap => self.colour_box(font, glyph),
            });
            each(glyph, b);
        }
    }
}

/// A glyph id as the caches key it: OpenType ids are below 0xFFFF (a face has at most 65535 glyphs), so anything
/// larger (a cmap format 12 entry may name any u32) is 0xFFFF, which no face has and nothing draws. A font can't then
/// spread a cache's keys over 32 bits, and the PDF writer's 16-bit glyph codes can't name a different glyph.
#[inline]
pub(crate) fn glyph_id(id: u32) -> u32 {
    id.min(0xFFFF)
}

/// The face's colour bitmap strikes (sbix, then CBDT), if it has any.
pub(crate) fn colour_strikes<'a>(f: &FontRef<'a>) -> Option<BitmapStrikes<'a>> {
    BitmapStrikes::with_format(f, skrifa::bitmap::BitmapFormat::Sbix).or_else(|| BitmapStrikes::with_format(f, skrifa::bitmap::BitmapFormat::Cbdt))
}

/// What draws a glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlyphKind {
    Outline,
    /// A COLRv0 or COLRv1 colour glyph.
    Colr,
    /// A CBDT or sbix bitmap.
    Bitmap,
}

/// A bitmap glyph's placement in em units, y down, from its bearings (font units) and inner bearings (strike pixels).
/// CBDT places a bitmap by its top left corner, sbix by its bottom left.
pub(crate) fn bitmap_em_box(b: &skrifa::bitmap::BitmapGlyph<'_>, upem: f64) -> kurbo::Rect {
    let (px, py) = (b.ppem_x.max(1.0) as f64, b.ppem_y.max(1.0) as f64);
    let (w, h) = (b.width as f64 / px, b.height as f64 / py);
    let x0 = b.bearing_x as f64 / upem + b.inner_bearing_x as f64 / px;
    let top = match b.placement_origin {
        skrifa::bitmap::Origin::TopLeft => b.bearing_y as f64 / upem + b.inner_bearing_y as f64 / py,
        skrifa::bitmap::Origin::BottomLeft => b.bearing_y as f64 / upem + b.inner_bearing_y as f64 / py + h,
    };
    kurbo::Rect::new(x0, -top, x0 + w, -top + h)
}

/// The box a COLR glyph paints in (font units, y up): the union of what each fill is clipped to.
struct BoundsPainter<'a> {
    fonts: &'a FontRegistry,
    font: FontId,
    upem: f64,
    transforms: Vec<kurbo::Affine>,
    clips: Vec<kurbo::Rect>,
    ink: Option<kurbo::Rect>,
}

impl BoundsPainter<'_> {
    fn clip(&mut self, b: kurbo::Rect) {
        let t = self.transforms.last().copied().unwrap_or_default();
        let b = t.transform_rect_bbox(b);
        let b = self.clips.last().map_or(b, |c| c.intersect(b));
        self.clips.push(b);
    }
}

impl skrifa::color::ColorPainter for BoundsPainter<'_> {
    fn push_transform(&mut self, t: skrifa::color::Transform) {
        let top = self.transforms.last().copied().unwrap_or_default();
        self.transforms.push(top * kurbo::Affine::new([t.xx, t.yx, t.xy, t.yy, t.dx, t.dy].map(|v| v as f64)));
    }
    fn pop_transform(&mut self) {
        self.transforms.pop();
    }
    fn push_clip_glyph(&mut self, glyph: GlyphId) {
        let mut outline = BezPath::new();
        let b = if self.fonts.em_outline(self.font, glyph.to_u32(), &mut outline) && !outline.is_empty() {
            let em = outline.control_box();
            kurbo::Rect::new(em.x0 * self.upem, -em.y1 * self.upem, em.x1 * self.upem, -em.y0 * self.upem)
        } else {
            kurbo::Rect::ZERO
        };
        self.clip(b);
    }
    fn push_clip_box(&mut self, b: skrifa::raw::types::BoundingBox<f32>) {
        self.clip(kurbo::Rect::new(b.x_min as f64, b.y_min as f64, b.x_max as f64, b.y_max as f64));
    }
    fn pop_clip(&mut self) {
        self.clips.pop();
    }
    fn fill(&mut self, _brush: skrifa::color::Brush<'_>) {
        if let Some(&c) = self.clips.last() {
            self.ink = Some(self.ink.map_or(c, |i| i.union(c)));
        }
    }
    fn push_layer(&mut self, _mode: skrifa::color::CompositeMode) {}
    fn pop_layer(&mut self) {}
}

/// skrifa's pen into a kurbo path, flipping y to point down.
struct Pen<'a>(&'a mut BezPath);

impl OutlinePen for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, -y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, -y as f64));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to((cx0 as f64, -cy0 as f64), (x as f64, -y as f64));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to((cx0 as f64, -cy0 as f64), (cx1 as f64, -cy1 as f64), (x as f64, -y as f64));
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

/// Synthetic bold's stroke width in em units for text of `size` (local units): Skia's fake-bold amount
/// (`kStdFakeBoldInterpKeys` 9 and 36, `kStdFakeBoldInterpValues` 1/24 and 1/32), interpolated on the text size.
pub(crate) fn embolden_em(size: f32) -> f32 {
    let (k0, k1, v0, v1) = (9.0, 36.0, 1.0 / 24.0, 1.0 / 32.0);
    if size <= k0 {
        v0
    } else if size >= k1 {
        v1
    } else {
        v0 + (v1 - v0) * (size - k0) / (k1 - k0)
    }
}

/// How a glyph's em-space outline is styled: synthetic italic's skew and synthetic bold's stroke width (em units, 0
/// for none).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Style {
    pub skew_x: f32,
    pub bold: f32,
}

/// How far, in em units, hinting moves a glyph's ink past its unhinted box, beyond one device pixel: snapping heights
/// to the pixel grid moves an edge by up to a pixel, and rounding the scale to fit the blue zones adds about a pixel
/// per 80 pixels per em. Measured over every glyph of a TrueType and a CFF face from 4 to 255 pixels per em, and held
/// by `hinted_ink_stays_within_its_reach`.
pub(crate) const HINT_REACH: f64 = 1.0 / 64.0;

/// Synthetic bold's miter limit: Skia's default, 4.
const BOLD_MITER_LIMIT: f64 = 4.0;

impl Style {
    /// The em-space transform synthetic italic applies, `x' = x + skew_x · y` with y down: tops (negative y) lean
    /// right for a negative `skew_x`, as Skia's `setSkewX(-0.2)` does.
    pub(crate) fn skew(self) -> kurbo::Affine {
        kurbo::Affine::new([1.0, 0.0, self.skew_x as f64, 1.0, 0.0, 0.0])
    }

    /// How far synthetic bold can grow a glyph's box, em units, on each side: half the pen's width, and at a sharp
    /// corner its miter, up to the pen's miter limit times that.
    pub fn outset(self) -> f64 {
        self.bold as f64 / 2.0 * BOLD_MITER_LIMIT
    }

    /// The pen whose stroke ring makes synthetic bold.
    pub(crate) fn pen(self) -> kurbo::Stroke {
        kurbo::Stroke::new(self.bold as f64).with_join(kurbo::Join::Miter).with_miter_limit(BOLD_MITER_LIMIT)
    }

    /// A glyph's em box (y down) as it is drawn: skewed, outset, and grown by [`HINT_REACH`]. Grown by one device pixel
    /// more, it holds all of the glyph's ink, hinted or not.
    pub fn ink(self, em: kurbo::Rect) -> kurbo::Rect {
        let grow = self.outset() + HINT_REACH;
        // Unskewed (most text), the box is the em box itself: the identity's bounding box, without mapping corners.
        let b = if self.skew_x == 0.0 { em.abs() } else { self.skew().transform_rect_bbox(em) };
        b.inflate(grow, grow)
    }
}

/// Scratch memory for turning glyphs into styled outlines, kept between glyphs.
#[derive(Debug, Default)]
pub(crate) struct Outliner {
    outline: BezPath,
    contour: BezPath,
    stroker: kurbo::StrokeCtx,
}

impl Outliner {
    /// Appends glyph `glyph`'s outline, styled by `style` in em space and then mapped by `map` (from em units), to
    /// `verbs` and `points`. `Ok(false)` for a glyph without ink (a space), `Err` when the face or glyph is missing or
    /// its outline doesn't read.
    #[allow(clippy::too_many_arguments, reason = "the glyph, its style and placement, and where it goes stay explicit")]
    pub fn glyph(
        &mut self,
        fonts: &FontRegistry,
        font: FontId,
        glyph: u32,
        style: Style,
        map: kurbo::Affine,
        verbs: &mut Vec<Verb>,
        points: &mut Vec<Point>,
    ) -> Result<bool, ()> {
        self.glyph_from(fonts, font, glyph, None, style, map, verbs, points)
    }

    /// As [`Outliner::glyph`], from `hinting`'s pixel-unit outline when given (then `map` takes pixels, not ems, and
    /// the bold width is still in ems of `hinting`'s size).
    #[allow(clippy::too_many_arguments, reason = "the glyph, its style and placement, and where it goes stay explicit")]
    fn glyph_from(
        &mut self,
        fonts: &FontRegistry,
        font: FontId,
        glyph: u32,
        hinting: Option<&HintingInstance>,
        style: Style,
        map: kurbo::Affine,
        verbs: &mut Vec<Verb>,
        points: &mut Vec<Point>,
    ) -> Result<bool, ()> {
        self.outline.truncate(0);
        let (read, unit) = match hinting {
            Some(h) => (fonts.hinted_outline(font, glyph, h, &mut self.outline), h.size().ppem().unwrap_or(1.0) as f64),
            None => (fonts.em_outline(font, glyph, &mut self.outline), 1.0),
        };
        if !read {
            return Err(());
        }
        if self.outline.is_empty() {
            return Ok(false);
        }
        let style = Style { bold: style.bold * unit as f32, ..style };
        let map = map * style.skew();
        crate::path::push_mapped(self.outline.elements(), map, verbs, points);
        if style.bold > 0.0 {
            // Each contour's stroke ring, turned to wind as the glyph does: inside the glyph the two add up, outside
            // the ring alone covers, so the non-zero fill of both is the glyph grown by half the width.
            let glyph_sign = self.outline.area().signum();
            let pen = style.pen();
            let mut start = 0;
            let els = self.outline.elements();
            while start < els.len() {
                let end = els[start + 1..].iter().position(|e| matches!(e, PathEl::MoveTo(_))).map_or(els.len(), |i| start + 1 + i);
                self.contour.truncate(0);
                self.contour.extend(els[start..end].iter().copied());
                kurbo::stroke_with(self.contour.iter(), &pen, &kurbo::StrokeOpts::default(), 1e-3, &mut self.stroker);
                let ring = self.stroker.output();
                if ring.area().signum() == glyph_sign {
                    crate::path::push_mapped(ring.elements(), map, verbs, points);
                } else {
                    crate::path::push_mapped(ring.reverse_subpaths().elements(), map, verbs, points);
                }
                start = end;
            }
        }
        Ok(true)
    }
}

/// What a glyph mask is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GlyphKey {
    pub font: FontId,
    pub glyph: u32,
    /// The em-to-device 2×2 matrix (size × the transform's linear part), in 1/64 pixel.
    pub matrix: [i32; 4],
    /// The glyph origin's position within its pixel, in quarters: x, and y (0 for axis-aligned text, whose baseline is
    /// rounded).
    pub bin: [u8; 2],
    /// Synthetic italic's skew and synthetic bold's em width, as `f32` bits.
    pub skew: u32,
    pub bold: u32,
    /// The text colour's luminance level (0 to 7), which picks the mask gamma.
    pub lum: u8,
}

/// Every field, packed into four words: one hasher step each, where hashing field by field takes ten (a page's glyphs
/// are looked up every frame).
impl std::hash::Hash for GlyphKey {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        let pair = |a: u32, b: u32| u64::from(a) | u64::from(b) << 32;
        let m = self.matrix.map(|v| v as u32);
        h.write_u64(pair(self.font.0, self.glyph));
        h.write_u64(pair(m[0], m[1]));
        h.write_u64(pair(m[2], m[3]));
        h.write_u64(pair(self.skew ^ self.bold.rotate_left(16), u32::from_le_bytes([self.bin[0], self.bin[1], self.lum, 0])));
    }
}

impl GlyphKey {
    pub fn style(self) -> Style {
        Style { skew_x: f32::from_bits(self.skew), bold: f32::from_bits(self.bold) }
    }
}

/// A glyph's coverage, `width × height`, its top left at (`left`, `top`) from the glyph origin's pixel.
#[derive(Debug, Default)]
pub(crate) struct GlyphMask {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    /// The face or glyph was missing or its outline didn't read (the mask is empty).
    pub failed: bool,
    /// The glyph is a colour glyph (COLR or bitmap): it draws from a colour mask instead (this one is empty).
    pub colour: bool,
    used: u64,
}

/// A strike's place in its [`GlyphCache`]: valid until the cache's next trim (a run's glyphs, within a frame).
#[derive(Clone, Copy, Debug)]
pub(crate) struct StrikeId(usize);

/// One strike's masks: a key with no glyph and no bin (what a run's glyphs share), and its masks by glyph and bin.
#[derive(Debug)]
struct Strike {
    key: GlyphKey,
    masks: FastMap<u64, GlyphMask>,
}

/// A glyph and its bin, as a strike's key for its mask.
fn in_strike(glyph: u32, bin: [u8; 2]) -> u64 {
    u64::from(glyph) << 4 | u64::from(bin[0]) << 2 | u64::from(bin[1])
}

/// Glyph masks by key, least recently used dropped past the byte budget. Masks are grouped in strikes, as Skia's are:
/// a run finds its strike once and each glyph is then a small key in a small map, where one map for every glyph made
/// each lookup a cache miss.
pub(crate) struct GlyphCache {
    strikes: Vec<Strike>,
    strike_ids: FastMap<GlyphKey, usize>,
    /// Light hinting instances by face and pixels per em (1/64), for text on the pixel grid (see [`hinted_ppem`]).
    hinters: FastMap<(FontId, i32), Option<HintingInstance>>,
    /// Whether masks of text on the pixel grid are hinted: always, except in tests that compare the two.
    pub(crate) hint: bool,
    clock: u64,
    bytes: usize,
    budget: usize,
    pub outliner: Outliner,
    verbs: Vec<Verb>,
    points: Vec<Point>,
}

impl fmt::Debug for GlyphCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlyphCache").field("strikes", &self.strikes.len()).field("bytes", &self.bytes).finish()
    }
}

impl Default for GlyphCache {
    fn default() -> Self {
        GlyphCache {
            strikes: Vec::new(),
            strike_ids: FastMap::default(),
            hinters: FastMap::default(),
            hint: true,
            clock: 0,
            bytes: 0,
            budget: 8 << 20,
            outliner: Outliner::default(),
            verbs: Vec::new(),
            points: Vec::new(),
        }
    }
}

impl GlyphCache {
    /// The strike for `key`'s run: everything in it but the glyph and the bin.
    pub fn strike(&mut self, key: GlyphKey) -> StrikeId {
        let key = GlyphKey { glyph: 0, bin: [0, 0], ..key };
        let strikes = &mut self.strikes;
        StrikeId(*self.strike_ids.entry(key).or_insert_with(|| {
            strikes.push(Strike { key, masks: FastMap::default() });
            strikes.len() - 1
        }))
    }

    /// The mask for `key`, rasterised now if it isn't cached. An unknown face or glyph gives an empty mask.
    #[cfg(test)]
    pub fn mask(&mut self, fonts: &FontRegistry, raster: &mut Rasterizer, key: GlyphKey) -> &GlyphMask {
        let strike = self.strike(key);
        self.mask_in(fonts, raster, strike, key.glyph, key.bin)
    }

    /// The mask for `glyph` at `bin` in `strike`, rasterised now if it isn't cached. An unknown face or glyph gives an
    /// empty mask.
    pub fn mask_in(&mut self, fonts: &FontRegistry, raster: &mut Rasterizer, strike: StrikeId, glyph: u32, bin: [u8; 2]) -> &GlyphMask {
        // Over the budget mid-frame, older masks go now (strikes stay, so ids hold until the frame's trim).
        if self.bytes > self.budget {
            self.drop_old();
        }
        self.clock += 1;
        let clock = self.clock;
        let GlyphCache { strikes, hinters, hint, bytes, outliner, verbs, points, .. } = self;
        let strike = &mut strikes[strike.0];
        let key = GlyphKey { glyph, bin, ..strike.key };
        let mask = strike.masks.entry(in_strike(glyph, bin)).or_insert_with(|| {
            let hinting = hinted_ppem(key)
                .filter(|_| *hint)
                .and_then(|ppem| hinters.entry((key.font, ppem)).or_insert_with(|| fonts.hinting(key.font, ppem as f32 / 64.0)).as_ref());
            let m = rasterize(fonts, raster, outliner, key, hinting, verbs, points);
            *bytes += m.data.len() + std::mem::size_of::<GlyphMask>();
            m
        });
        mask.used = clock;
        mask
    }

    /// Drops every mask and hinting instance: they were another registry's faces'.
    pub fn clear(&mut self) {
        self.strikes.clear();
        self.strike_ids.clear();
        self.hinters.clear();
        self.bytes = 0;
    }

    /// Drops the least recently used masks while the cache is over its budget (after a frame), down to half of it.
    pub fn trim(&mut self) {
        // ponytail: hinting instances (one per face and size, so a zoom sweep makes hundreds) aren't in the byte
        // budget; past 64 they all go and are made again as drawn. Count their bytes if that churn ever shows.
        if self.hinters.len() > 64 {
            self.hinters.clear();
        }
        if self.bytes <= self.budget {
            return;
        }
        self.drop_old();
        // Empty strikes go too, which moves the others: their ids are renumbered.
        self.strikes.retain(|s| !s.masks.is_empty());
        self.strike_ids = self.strikes.iter().enumerate().map(|(i, s)| (s.key, i)).collect();
    }

    /// Drops the least recently used masks down to half the budget, keeping every strike (and so every id).
    fn drop_old(&mut self) {
        let mut by_age: Vec<(u64, usize, u64)> =
            self.strikes.iter().enumerate().flat_map(|(i, s)| s.masks.iter().map(move |(&k, m)| (m.used, i, k))).collect();
        by_age.sort_unstable_by_key(|&(used, ..)| used);
        for (_, i, key) in by_age {
            if self.bytes <= self.budget / 2 {
                break;
            }
            if let Some(m) = self.strikes[i].masks.remove(&key) {
                self.bytes -= m.data.len() + std::mem::size_of::<GlyphMask>();
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.strikes.iter().map(|s| s.masks.len()).sum()
    }

    #[cfg(test)]
    fn has(&self, key: GlyphKey) -> bool {
        let strike = GlyphKey { glyph: 0, bin: [0, 0], ..key };
        self.strike_ids.get(&strike).is_some_and(|&i| self.strikes[i].masks.contains_key(&in_strike(key.glyph, key.bin)))
    }
}

/// The pixels per em (1/64) `key`'s mask is hinted at: text turned by a multiple of a quarter turn at a uniform scale,
/// without skew, whose glyph y axis lies along a device axis. `None` for any other transform, as hinting assumes the
/// pixel grid.
fn hinted_ppem(key: GlyphKey) -> Option<i32> {
    let [a, b, c, d] = key.matrix;
    (a == d && b == -c && (a == 0) != (b == 0) && key.skew == 0).then(|| a.saturating_abs().saturating_add(b.saturating_abs()))
}

/// A glyph origin's device coordinate `v` in quarter pixels, rounded to a whole pixel when `whole`: the baseline's
/// coordinate across the run (y for axis-aligned text, x for text turned a quarter), as Skia rounds it: halves up,
/// so a coordinate moved by whole pixels rounds moved by them.
#[inline]
pub(crate) fn quarters(v: f32, whole: bool) -> i32 {
    if whole { ((v + 0.5).floor() as i32).saturating_mul(4) } else { (v * 4.0 + 0.5).floor() as i32 }
}

#[allow(clippy::too_many_arguments, reason = "the mask's inputs and scratch stay explicit")]
fn rasterize(
    fonts: &FontRegistry,
    raster: &mut Rasterizer,
    outliner: &mut Outliner,
    key: GlyphKey,
    hinting: Option<&HintingInstance>,
    verbs: &mut Vec<Verb>,
    points: &mut Vec<Point>,
) -> GlyphMask {
    verbs.clear();
    points.clear();
    if fonts.kind(key.font, key.glyph) != GlyphKind::Outline {
        return GlyphMask { colour: true, ..GlyphMask::default() };
    }
    let [a, b, c, d] = key.matrix.map(|v| v as f64 / 64.0);
    let (dx, dy) = (key.bin[0] as f64 / 4.0, key.bin[1] as f64 / 4.0);
    // A hinted outline is already in pixels: only its quarter turn is left.
    let map = match hinting {
        Some(h) => {
            let s = h.size().ppem().unwrap_or(1.0) as f64;
            kurbo::Affine::new([a / s, b / s, c / s, d / s, dx, dy])
        }
        None => kurbo::Affine::new([a, b, c, d, dx, dy]),
    };
    match outliner.glyph_from(fonts, key.font, key.glyph, hinting, key.style(), map, verbs, points) {
        Ok(true) if !points.is_empty() => {}
        Ok(_) => return GlyphMask::default(),
        Err(()) => return GlyphMask { failed: true, ..GlyphMask::default() },
    }
    let (x0, y0, x1, y1) =
        points.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |(a, b, c, d), p| (a.min(p.x), b.min(p.y), c.max(p.x), d.max(p.y)));
    let bounds = IRect { x0: x0.floor() as i32, y0: y0.floor() as i32, x1: x1.ceil() as i32, y1: y1.ceil() as i32 };
    let (w, h) = (bounds.width().max(0) as usize, (bounds.y1 - bounds.y0).max(0) as usize);
    // An em at most 256 pixels across makes masks far smaller than this; anything larger is a broken outline, which
    // draws nothing rather than taking unbounded memory.
    let em = crate::cover::max_scale([a, b, c, d]);
    if w > 4096 || h > 4096 || !within_reach(bounds, glyph_reach(em, OUTLINE_EMS)) {
        return GlyphMask::default();
    }
    let mut data = vec![0u8; w * h];
    raster.mask(verbs, points, bounds, &mut data, &gamma_tables()[key.lum as usize]);
    GlyphMask { left: bounds.x0, top: bounds.y0, width: w as u32, height: h as u32, data, failed: false, colour: false, used: 0 }
}

/// How far an outline glyph's ink may reach from its origin, in ems, and a colour glyph's (emoji fill about one).
pub(crate) const OUTLINE_EMS: f64 = 8.0;
pub(crate) const COLOUR_EMS: f64 = 2.0;

/// How far, in pixels either way, a glyph's mask may reach from its origin's pixel at `em` pixels per em: `ems` ems,
/// plus the subpixel offset and rounding. A mask past it is a broken glyph (or a font's trap: a huge clip box at a
/// small size) and draws nothing, so a glyph that far off the clip can be skipped unseen.
pub(crate) fn glyph_reach(em: f64, ems: f64) -> i32 {
    (ems * em).ceil() as i32 + 2
}

/// Whether `b` (pixels from the glyph origin's pixel) is within `reach` of it.
pub(crate) fn within_reach(b: IRect, reach: i32) -> bool {
    b.x0 >= -reach && b.y0 >= -reach && b.x1 <= reach && b.y1 <= reach
}

/// The luminance level (0 to 7) of a premultiplied colour, which picks the mask gamma: its sRGB luminance (Rec. 709
/// weights on linear light), as Skia computes a text colour's, in 3 bits.
pub(crate) fn luminance_level(r: u8, g: u8, b: u8, a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    let lin = |c: u8| srgb_to_linear((c as f32 / a as f32).min(1.0));
    let luma = 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
    ((linear_to_srgb(luma) * 255.0).round() as u32 >> 5) as u8
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(l: f32) -> f32 {
    if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 }
}

/// Skia's A8 mask gamma tables, one per luminance level: `SkTMaskGamma_build_correcting_lut` with contrast 0.5 and
/// sRGB luminance on both the text and the destination side.
pub(crate) fn gamma_tables() -> &'static [[u8; 256]; 8] {
    static TABLES: OnceLock<[[u8; 256]; 8]> = OnceLock::new();
    TABLES.get_or_init(|| {
        const CONTRAST: f32 = 0.5;
        let mut tables = [[0u8; 256]; 8];
        for (level, table) in tables.iter_mut().enumerate() {
            // The level's luminance as 8 bits (its 3 bits repeated, Skia's `sk_t_scale255<3>`).
            let base = (level as u32) << 5;
            let src = (base | base >> 3 | base >> 6) as f32 / 255.0;
            let lin_src = srgb_to_linear(src);
            // Skia guesses the destination as the perceptual inverse of the text.
            let dst = 1.0 - src;
            let lin_dst = srgb_to_linear(dst);
            let contrast = CONTRAST * lin_dst;
            for (i, out) in table.iter_mut().enumerate() {
                let raw = i as f32 / 255.0;
                let srca = raw + (1.0 - raw) * contrast * raw;
                let v = if (src - dst).abs() < 1.0 / 256.0 {
                    srca
                } else {
                    let lin_out = lin_src * srca + (1.0 - srca) * lin_dst;
                    // Undo what 8-bit blending will do.
                    (linear_to_srgb(lin_out) - dst) / (src - dst)
                };
                *out = (255.0 * v + 0.5).floor().clamp(0.0, 255.0) as u8;
            }
        }
        tables
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fearless_simd::Level;

    fn mono() -> FontData {
        Arc::new(include_bytes!("../tests/fonts/jetbrains-mono-500.ttf").to_vec())
    }

    fn glyph(font: &FontData, c: char) -> u32 {
        let f = FontRef::new((**font).as_ref()).expect("the test font reads");
        f.charmap().map(c).expect("the test font has the character").to_u32()
    }

    fn key(font: FontId, glyph: u32, px: i32) -> GlyphKey {
        GlyphKey { font, glyph, matrix: [px * 64, 0, 0, px * 64], bin: [0, 0], skew: 0, bold: 0, lum: 0 }
    }

    fn ink(m: &[u8]) -> u32 {
        m.iter().map(|&c| c as u32).sum()
    }

    #[test]
    fn faces_register_once_and_bad_bytes_fail() {
        let mut fonts = FontRegistry::default();
        let data = mono();
        let a = fonts.add(data.clone(), 0, &[]).expect("a face");
        assert_eq!(fonts.add(data.clone(), 0, &[]), Ok(a));
        assert_ne!(fonts.add(data.clone(), 0, &[(*b"wght", 700.0)]), Ok(a), "other coordinates are another face");
        assert!(fonts.add(Arc::new(vec![1, 2, 3]), 0, &[]).is_err());
        assert!(fonts.add(data, 5, &[]).is_err(), "no face 5 in a single-face file");
    }

    #[test]
    fn masks_are_cached_by_key() {
        let mut fonts = FontRegistry::default();
        let data = mono();
        let font = fonts.add(data.clone(), 0, &[]).expect("a face");
        let mut raster = Rasterizer::new(Level::new());
        let mut cache = GlyphCache::default();
        let k = key(font, glyph(&data, 'a'), 16);
        let plain = cache.mask(&fonts, &mut raster, k).data.clone();
        assert!(plain.contains(&255), "a 16 px 'a' has fully covered pixels");
        assert_eq!(cache.mask(&fonts, &mut raster, k).data, plain);
        assert_eq!(cache.len(), 1, "the second request hit the cache");
        assert_ne!(cache.mask(&fonts, &mut raster, GlyphKey { bin: [2, 0], ..k }).data, plain, "half a pixel right");
        let space = GlyphKey { glyph: glyph(&data, ' '), ..k };
        assert_eq!(cache.mask(&fonts, &mut raster, space).width, 0, "a space has no ink");
        assert_eq!(cache.mask(&fonts, &mut raster, GlyphKey { font: FontId(99), ..k }).width, 0);
    }

    #[test]
    fn rotated_and_flipped_masks_hold_the_same_ink() {
        let mut fonts = FontRegistry::default();
        let data = mono();
        let font = fonts.add(data.clone(), 0, &[]).expect("a face");
        let mut raster = Rasterizer::new(Level::new());
        // Unhinted throughout, so only the transform differs.
        let mut cache = GlyphCache { hint: false, ..GlyphCache::default() };
        let k = key(font, glyph(&data, 'R'), 24);
        let upright = ink(&cache.mask(&fonts, &mut raster, k).data);
        let quarter = GlyphKey { matrix: [0, 24 * 64, -24 * 64, 0], ..k };
        let turned = cache.mask(&fonts, &mut raster, quarter);
        assert!(turned.width > turned.height, "an R is taller than wide, so turned a quarter it is wider");
        let turned = ink(&turned.data);
        let flipped = ink(&cache.mask(&fonts, &mut raster, GlyphKey { matrix: [-24 * 64, 0, 0, 24 * 64], ..k }).data);
        for other in [turned, flipped] {
            assert!(other.abs_diff(upright) * 50 < upright, "{upright} vs {other}");
        }
    }

    #[test]
    fn synthetic_bold_grows_the_glyph_and_keeps_its_counters() {
        let mut fonts = FontRegistry::default();
        let data = mono();
        let font = fonts.add(data.clone(), 0, &[]).expect("a face");
        let mut raster = Rasterizer::new(Level::new());
        let mut cache = GlyphCache::default();
        let o = key(font, glyph(&data, 'O'), 48);
        let plain = cache.mask(&fonts, &mut raster, o);
        let (w, h, plain_ink) = (plain.width, plain.height, ink(&plain.data));
        let bold = cache.mask(&fonts, &mut raster, GlyphKey { bold: embolden_em(36.0).to_bits(), ..o });
        assert!(bold.width > w && bold.height > h, "the outset grows the box");
        assert!(ink(&bold.data) > plain_ink * 11 / 10, "and adds ink");
        // The O's counter stays open: its middle is uncovered.
        let mid = (bold.height / 2 * bold.width + bold.width / 2) as usize;
        assert_eq!(bold.data[mid], 0, "the counter is not filled");
        // And the ring adds ink inside the stroke, not holes in it.
        let row = &bold.data[(bold.height / 2 * bold.width) as usize..][..bold.width as usize];
        assert!(row.iter().filter(|&&c| c == 255).count() >= 4, "{row:?}");
    }

    #[test]
    fn glyph_ids_past_a_faces_are_one_that_draws_nothing() {
        // A cmap format 12 entry may name any u32: the caches (and the unmixed hash, `fx`) see at most 0xFFFF.
        assert_eq!([0, 0xFFFE, 0xFFFF, 0x1_0041, u32::MAX].map(glyph_id), [0, 0xFFFE, 0xFFFF, 0xFFFF, 0xFFFF]);
    }

    #[test]
    fn boxes_are_exact_and_remembered() {
        let mut fonts = FontRegistry::default();
        let data = mono();
        let font = fonts.add(data.clone(), 0, &[]).expect("a face");
        let mut seen = Vec::new();
        fonts.boxes(font, [glyph(&data, 'g'), glyph(&data, ' ')].into_iter(), |_, b| seen.push(b));
        let g = seen[0].expect("a g has ink");
        assert!(g.y1 > 0.1 && g.y0 < -0.4, "a g descends below the baseline and rises above it: {g:?}");
        assert_eq!(seen[1], None, "a space has none");
        assert_eq!(fonts.boxes.lock().expect("unpoisoned").len(), 2);
    }

    #[test]
    fn outlines_are_read_once_per_glyph() {
        let mut fonts = FontRegistry::default();
        let data = mono();
        let font = fonts.add(data.clone(), 0, &[]).expect("a face");
        let (g, space) = (glyph(&data, 'g'), glyph(&data, ' '));
        let mut seen = Vec::new();
        for _ in 0..2 {
            fonts.outlines(font, [g, space, g, u32::MAX].into_iter(), |k, o| seen.push((k, o.elements().len())));
        }
        assert_eq!(seen.iter().map(|s| s.0).collect::<Vec<_>>(), [0, 2, 0, 2], "a space and a glyph past the face's have none");
        assert!(seen[0].1 > 4);
        assert_eq!(fonts.outlines.lock().expect("unpoisoned").len(), 3, "g, the space, and every id past the face's as one");
    }

    #[test]
    fn the_cache_trims_to_its_budget_oldest_first() {
        let mut fonts = FontRegistry::default();
        let data = mono();
        let font = fonts.add(data.clone(), 0, &[]).expect("a face");
        let mut raster = Rasterizer::new(Level::new());
        let mut cache = GlyphCache { budget: 4096, ..GlyphCache::default() };
        let w = glyph(&data, 'W');
        for px in 10..40 {
            cache.mask(&fonts, &mut raster, key(font, w, px));
        }
        cache.trim();
        assert!(cache.bytes <= 4096);
        assert!(cache.has(key(font, w, 39)), "the newest survives");
        assert!(!cache.has(key(font, w, 10)), "the oldest goes");
        // The strikes left were renumbered: a survivor is still found, and a dropped one is made again.
        let (n, bytes) = (cache.len(), cache.bytes);
        let survivor = cache.mask(&fonts, &mut raster, key(font, w, 39)).data.clone();
        assert_eq!((cache.len(), cache.bytes), (n, bytes), "a hit");
        assert_eq!(survivor, GlyphCache::default().mask(&fonts, &mut raster, key(font, w, 39)).data);
        assert!(cache.mask(&fonts, &mut raster, key(font, w, 10)).width > 0);
        assert_eq!(cache.len(), n + 1);
    }

    /// The mask `cache` makes for `k`: its left, top, width, height and coverage.
    fn placed(cache: &mut GlyphCache, fonts: &FontRegistry, raster: &mut Rasterizer, k: GlyphKey) -> (i32, i32, u32, u32, Vec<u8>) {
        let m = cache.mask(fonts, raster, k);
        (m.left, m.top, m.width, m.height, m.data.clone())
    }

    #[test]
    fn a_quarter_turned_mask_is_the_hinted_upright_mask_turned() {
        let mut fonts = FontRegistry::default();
        let data = mono();
        let font = fonts.add(data.clone(), 0, &[]).expect("a face");
        let mut raster = Rasterizer::new(Level::new());
        let mut cache = GlyphCache::default();
        for (c, px) in [('E', 11), ('g', 16), ('R', 24), ('&', 13)] {
            let upright = key(font, glyph(&data, c), px);
            let u = placed(&mut cache, &fonts, &mut raster, upright);
            // Turned a quarter clockwise (y down): device (x, y) is the upright glyph's (y, -x).
            let t = placed(&mut cache, &fonts, &mut raster, GlyphKey { matrix: [0, px * 64, -px * 64, 0], ..upright });
            assert_eq!((t.0, t.1, t.2, t.3), (-(u.1 + u.3 as i32), u.0, u.3, u.2), "'{c}' at {px} px: the box turned");
            // The same coverage, but where an edge on a pixel boundary rounds the other way across rows than columns.
            let mut off = 0;
            for y in 0..u.3 {
                for x in 0..u.2 {
                    let turned = t.4[(x * t.2 + (u.3 - 1 - y)) as usize];
                    off += usize::from(u.4[(y * u.2 + x) as usize].abs_diff(turned) > 2);
                }
            }
            assert!(off * 100 <= u.4.len(), "'{c}' at {px} px: {off} pixels differ");
            // Turned a half and three quarters too, and none of them is the unhinted mask.
            for matrix in [[-px * 64, 0, 0, -px * 64], [0, -px * 64, px * 64, 0]] {
                let k = GlyphKey { matrix, ..upright };
                let hinted = placed(&mut cache, &fonts, &mut raster, k);
                assert_ne!(hinted, placed(&mut GlyphCache { hint: false, ..GlyphCache::default() }, &fonts, &mut raster, k));
                assert_eq!(ink(&hinted.4).abs_diff(ink(&u.4)) * 100 / ink(&u.4), 0, "'{c}' at {px} px, {matrix:?}");
            }
        }
        // Text at any other angle, mirrored or skewed is not hinted: the switch doesn't change it.
        let e = key(font, glyph(&data, 'E'), 11);
        for k in [
            GlyphKey { matrix: [500, 300, -300, 500], ..e },
            GlyphKey { matrix: [-11 * 64, 0, 0, 11 * 64], ..e },
            GlyphKey { matrix: [0, 11 * 64, 11 * 64, 0], ..e },
            GlyphKey { skew: (-0.2f32).to_bits(), ..e },
        ] {
            let mut plain = GlyphCache { hint: false, ..GlyphCache::default() };
            assert_eq!(placed(&mut cache, &fonts, &mut raster, k), placed(&mut plain, &fonts, &mut raster, k), "{k:?}");
        }
        // Down to a 64th of a pixel per em, every way round, and at the largest em a mask is made for: no panic.
        for v in [1, 2, 63, 256 * 64] {
            for matrix in [[0, v, -v, 0], [0, -v, v, 0], [-v, 0, 0, -v], [v, 0, 0, v]] {
                let m = cache.mask(&fonts, &mut raster, GlyphKey { matrix, ..e });
                assert!(m.width <= 4096 && m.height <= 4096);
            }
        }
    }

    /// Hinting is vertical-only and keeps linear metrics: along the baseline no point of a glyph the gate sets moves
    /// more than half a pixel, so its origin and advance stay where layout put them.
    #[test]
    fn hinting_keeps_a_glyphs_columns() {
        let mut fonts = FontRegistry::default();
        let data = mono();
        let font = fonts.add(data.clone(), 0, &[]).expect("a face");
        let f = FontRef::new((*data).as_ref()).expect("the test font reads");
        let glyphs: Vec<u32> = "EgR&aeHfi".chars().filter_map(|c| f.charmap().map(c)).map(|g| g.to_u32()).collect();
        assert!(glyphs.len() >= 4, "the test face has the gate's glyphs");
        let points = |path: &BezPath| -> Vec<kurbo::Point> {
            use kurbo::PathEl::*;
            path.elements()
                .iter()
                .flat_map(|e| match *e {
                    MoveTo(p) | LineTo(p) => vec![p],
                    QuadTo(a, p) => vec![a, p],
                    CurveTo(a, b, p) => vec![a, b, p],
                    ClosePath => Vec::new(),
                })
                .collect()
        };
        for px in [9, 11, 13, 16, 20, 24, 36] {
            let hinting = fonts.hinting(font, px as f32).expect("the face hints");
            for &glyph in &glyphs {
                let (mut plain, mut hinted) = (BezPath::new(), BezPath::new());
                assert!(fonts.em_outline(font, glyph, &mut plain) && fonts.hinted_outline(font, glyph, &hinting, &mut hinted));
                plain.apply_affine(kurbo::Affine::scale(px as f64));
                assert_eq!(plain.elements().len(), hinted.elements().len(), "glyph {glyph} at {px} px keeps its outline's shape");
                for (a, b) in points(&plain).iter().zip(points(&hinted)) {
                    assert!((a.x - b.x).abs() <= 0.5, "glyph {glyph} at {px} px: x {} moved to {}", a.x, b.x);
                }
            }
        }
    }

    #[test]
    fn hinted_ink_stays_within_its_reach() {
        // Every glyph of a TrueType and a CFF face, plain and bold, at sizes across the hinted range: the ink stays
        // inside the box `Style::ink` gives, rounded out to pixels and grown by one.
        for data in [mono(), Arc::new(font_test_data::NOTO_SANS_JP_CFF.to_vec()) as FontData] {
            let mut fonts = FontRegistry::default();
            let font = fonts.add(data.clone(), 0, &[]).expect("a face");
            let n = FontRef::new((*data).as_ref()).expect("reads").maxp().expect("maxp").num_glyphs() as u32;
            let mut boxes = Vec::new();
            fonts.boxes(font, 0..n, |g, b| boxes.extend(b.map(|b| (g, b))));
            let mut raster = Rasterizer::new(Level::new());
            for px in [4, 5, 7, 9, 11, 13, 16, 20, 27, 36, 48, 64, 83, 104, 128, 153, 180, 210, 242, 255] {
                let mut cache = GlyphCache::default();
                for bold in [0.0, embolden_em(px as f32)] {
                    let style = Style { skew_x: 0.0, bold };
                    for &(glyph, em) in &boxes {
                        let k = GlyphKey { bold: bold.to_bits(), ..key(font, glyph, px) };
                        let m = cache.mask(&fonts, &mut raster, k);
                        let b = style.ink(em).scale_from_origin(px as f64);
                        let allowed = (b.x0.floor() as i32 - 1, b.y0.floor() as i32 - 1, b.x1.ceil() as i32 + 1, b.y1.ceil() as i32 + 1);
                        let w = m.width as usize;
                        for (i, _) in m.data.iter().enumerate().filter(|&(_, &c)| c > 0) {
                            let (x, y) = (m.left + (i % w) as i32, m.top + (i / w) as i32);
                            assert!(
                                x >= allowed.0 && y >= allowed.1 && x < allowed.2 && y < allowed.3,
                                "glyph {glyph} at {px} px, bold {bold}: ink at ({x}, {y}) outside {allowed:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn gamma_keeps_the_ends_and_order() {
        for table in gamma_tables() {
            assert_eq!((table[0], table[255]), (0, 255));
            assert!(table.windows(2).all(|w| w[0] <= w[1]), "{table:?}");
        }
        // Black text keeps less of a half-covered pixel than white text does: the same weight after blending.
        let (black, white) = (gamma_tables()[0][128], gamma_tables()[7][128]);
        assert!(black < 128 && white > 128, "{black} {white}");
    }

    #[test]
    fn luminance_levels_span_black_to_white() {
        assert_eq!(luminance_level(0, 0, 0, 255), 0);
        assert_eq!(luminance_level(255, 255, 255, 255), 7);
        assert_eq!(luminance_level(128, 128, 128, 255), 4);
        assert_eq!(luminance_level(64, 64, 64, 128), 4, "unpremultiplied first");
    }

    #[test]
    fn embolden_follows_skias_curve() {
        assert_eq!(embolden_em(9.0), 1.0 / 24.0);
        assert_eq!(embolden_em(36.0), 1.0 / 32.0);
        assert!((embolden_em(22.5) - (1.0 / 24.0 + 1.0 / 32.0) / 2.0).abs() < 1e-7);
    }
}
