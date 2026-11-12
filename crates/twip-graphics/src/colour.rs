//! Colour glyphs: a COLRv0 or COLRv1 glyph recorded as a picture through skrifa's `ColorPainter`, and a bitmap glyph
//! (CBDT, sbix) as pixels. A backend draws either into an RGBA glyph mask, cached like the coverage masks.
//!
//! The COLR picture is in em units, y down, so it is placed like any glyph. Its paint graph maps onto the recording
//! API one to one: transforms, clips to glyph outlines and boxes, fills with solid, linear, two-point conical and sweep
//! brushes (stops from the CPAL palette, the text colour for the foreground entry), and layers with a composite mode.

use kurbo::{Affine, BezPath, PathEl};
use skrifa::color::{Brush as ColrBrush, ColorPainter, ColorStop, CompositeMode, Extend as ColrExtend, Transform as ColrTransform};
use skrifa::raw::TableProvider;
use skrifa::raw::types::BoundingBox;
use skrifa::{GlyphId, MetadataProvider};

use crate::font::{FontId, FontRegistry, bitmap_em_box};
use crate::image::Pixmap;
use crate::paint::{BlendMode, Brush, Color, Extend, Paint, Stop};
use crate::scene::{Clip, Layer, Picture, Recorder};

/// Glyph `glyph` of `font` as a picture in em units, y down, with `foreground` for the palette's foreground entry;
/// `None` when it isn't a COLR glyph or its paint graph doesn't read.
pub(crate) fn colr_picture(fonts: &FontRegistry, font: FontId, glyph: u32, foreground: Color) -> Option<Picture> {
    let (f, location) = fonts.font_ref(font)?;
    let g = f.color_glyphs().get(GlyphId::new(glyph))?;
    let upem = f.head().ok()?.units_per_em() as f64;
    let palettes = f.color_palettes();
    let palette = palettes.get(0);
    let colours: Vec<Color> = palette.map_or(Vec::new(), |p| p.colors().iter().map(|c| Color::rgba(c.red, c.green, c.blue, c.alpha)).collect());
    let mut rec = Recorder::new();
    // The paint graph is in font units, y up.
    rec.push_transform(Affine::new([1.0 / upem, 0.0, 0.0, -1.0 / upem, 0.0, 0.0]));
    let mut painter = Painter { rec: &mut rec, fonts, font, upem, colours, foreground, outline: BezPath::new(), stops: Vec::new() };
    g.paint(location, &mut painter).ok()?;
    rec.pop();
    Some(rec.finish())
}

struct Painter<'a> {
    rec: &'a mut Recorder,
    fonts: &'a FontRegistry,
    font: FontId,
    upem: f64,
    colours: Vec<Color>,
    foreground: Color,
    outline: BezPath,
    stops: Vec<Stop>,
}

impl Painter<'_> {
    /// Palette entry `index` at `alpha` times its own; 0xFFFF is the text colour.
    fn colour(&self, index: u16, alpha: f32) -> Color {
        let c = if index == 0xFFFF { self.foreground } else { self.colours.get(index as usize).copied().unwrap_or(Color::BLACK) };
        c.with_alpha_f(c.a as f32 / 255.0 * alpha)
    }

    fn stops(&mut self, stops: &[ColorStop]) {
        let mut out = std::mem::take(&mut self.stops);
        out.clear();
        out.extend(stops.iter().map(|s| Stop { offset: s.offset, color: self.colour(s.palette_index, s.alpha) }));
        self.stops = out;
    }
}

fn extend(e: ColrExtend) -> Extend {
    match e {
        ColrExtend::Repeat => Extend::Repeat,
        ColrExtend::Reflect => Extend::Mirror,
        // Pad, and any mode a newer font defines: COLR says unknown modes pad.
        _ => Extend::Clamp,
    }
}

fn blend(mode: CompositeMode) -> BlendMode {
    match mode {
        CompositeMode::Clear => BlendMode::Clear,
        CompositeMode::Src => BlendMode::Src,
        CompositeMode::Dest => BlendMode::Dst,
        CompositeMode::SrcOver => BlendMode::SrcOver,
        CompositeMode::DestOver => BlendMode::DstOver,
        CompositeMode::SrcIn => BlendMode::SrcIn,
        CompositeMode::DestIn => BlendMode::DstIn,
        CompositeMode::SrcOut => BlendMode::SrcOut,
        CompositeMode::DestOut => BlendMode::DstOut,
        CompositeMode::SrcAtop => BlendMode::SrcAtop,
        CompositeMode::DestAtop => BlendMode::DstAtop,
        CompositeMode::Xor => BlendMode::Xor,
        CompositeMode::Plus => BlendMode::Plus,
        CompositeMode::Screen => BlendMode::Screen,
        CompositeMode::Overlay => BlendMode::Overlay,
        CompositeMode::Darken => BlendMode::Darken,
        CompositeMode::Lighten => BlendMode::Lighten,
        CompositeMode::ColorDodge => BlendMode::ColorDodge,
        CompositeMode::ColorBurn => BlendMode::ColorBurn,
        CompositeMode::HardLight => BlendMode::HardLight,
        CompositeMode::SoftLight => BlendMode::SoftLight,
        CompositeMode::Difference => BlendMode::Difference,
        CompositeMode::Exclusion => BlendMode::Exclusion,
        CompositeMode::Multiply => BlendMode::Multiply,
        CompositeMode::HslHue => BlendMode::Hue,
        CompositeMode::HslSaturation => BlendMode::Saturation,
        CompositeMode::HslColor => BlendMode::Color,
        CompositeMode::HslLuminosity => BlendMode::Luminosity,
        // A mode a newer font defines: COLR says to draw it as source-over.
        _ => BlendMode::SrcOver,
    }
}

impl ColorPainter for Painter<'_> {
    fn push_transform(&mut self, t: ColrTransform) {
        self.rec.push_transform(Affine::new([t.xx, t.yx, t.xy, t.yy, t.dx, t.dy].map(|v| v as f64)));
    }

    fn pop_transform(&mut self) {
        self.rec.pop();
    }

    fn push_clip_glyph(&mut self, glyph: GlyphId) {
        self.outline.truncate(0);
        if !self.fonts.em_outline(self.font, glyph.to_u32(), &mut self.outline) {
            self.outline.truncate(0);
        }
        // Back from em units, y down, into the graph's font units, y up.
        let to_units = Affine::new([self.upem, 0.0, 0.0, -self.upem, 0.0, 0.0]);
        let els: Vec<PathEl> = self.outline.elements().iter().map(|&e| to_units * e).collect();
        self.rec.push_clip(Clip::Path(&els));
    }

    fn push_clip_box(&mut self, b: BoundingBox<f32>) {
        let rect = kurbo::Rect::new(b.x_min as f64, b.y_min as f64, b.x_max as f64, b.y_max as f64);
        self.rec.push_clip(Clip::Rect { rect, aa: true });
    }

    fn pop_clip(&mut self) {
        self.rec.pop();
    }

    fn fill(&mut self, brush: ColrBrush<'_>) {
        // A fill paints everything inside the current clips; this rect reaches past any glyph.
        let everywhere = kurbo::Rect::new(-1e6, -1e6, 1e6, 1e6);
        let brush = match brush {
            ColrBrush::Solid { palette_index, alpha } => return self.rec.fill_rect(everywhere, self.colour(palette_index, alpha)),
            ColrBrush::LinearGradient { p0, p1, color_stops, extend: e } => {
                self.stops(color_stops);
                let (start, end) = ((p0.x as f64, p0.y as f64).into(), (p1.x as f64, p1.y as f64).into());
                Brush::Linear { start, end, stops: &self.stops, extend: extend(e), transform: Affine::IDENTITY }
            }
            ColrBrush::RadialGradient { c0, r0, c1, r1, color_stops, extend: e } => {
                self.stops(color_stops);
                Brush::Radial {
                    start: (c0.x as f64, c0.y as f64).into(),
                    start_radius: r0 as f64,
                    end: (c1.x as f64, c1.y as f64).into(),
                    end_radius: r1 as f64,
                    stops: &self.stops,
                    extend: extend(e),
                    transform: Affine::IDENTITY,
                }
            }
            ColrBrush::SweepGradient { c0, start_angle, end_angle, color_stops, extend: e } => {
                self.stops(color_stops);
                // skrifa's angles are clockwise as the glyph is seen; the sweep brush measures clockwise with y down,
                // so it works in the graph's space flipped upright.
                Brush::Sweep {
                    center: (c0.x as f64, -c0.y as f64).into(),
                    start_angle: start_angle as f64,
                    end_angle: end_angle as f64,
                    stops: &self.stops,
                    extend: extend(e),
                    transform: Affine::FLIP_Y,
                }
            }
        };
        self.rec.fill_rect(everywhere, Paint::new(brush));
    }

    fn push_layer(&mut self, mode: CompositeMode) {
        self.rec.push_layer(&Layer { blend: blend(mode), ..Layer::default() });
    }

    fn pop_layer(&mut self) {
        self.rec.pop();
    }
}

/// Glyph `glyph`'s bitmap from the strike nearest `ppem` (premultiplied RGBA) and where it goes, in em units, y down;
/// `None` when it has none or it doesn't decode. A mask bitmap takes `foreground`.
pub(crate) fn bitmap(fonts: &FontRegistry, font: FontId, glyph: u32, ppem: f32, foreground: Color) -> Option<(Pixmap, kurbo::Rect)> {
    let (f, _) = fonts.font_ref(font)?;
    let upem = f.head().ok()?.units_per_em() as f64;
    let b = crate::font::colour_strikes(&f)?.glyph_for_size(skrifa::instance::Size::new(ppem), GlyphId::new(glyph))?;
    let place = bitmap_em_box(&b, upem);
    let (w, h) = (b.width, b.height);
    let pixels = match &b.data {
        skrifa::bitmap::BitmapData::Png(png) => crate::image::decode(png).ok()?,
        skrifa::bitmap::BitmapData::Bgra(bgra) => {
            let data: Vec<u8> = bgra.as_chunks::<4>().0.iter().flat_map(|&[b, g, r, a]| [r, g, b, a]).collect();
            (data.len() == (w * h * 4) as usize).then_some(Pixmap { width: w, height: h, data })?
        }
        skrifa::bitmap::BitmapData::Mask(mask) => {
            let mut alpha = vec![0u8; (w * h) as usize];
            mask.decode_to_slice(w, h, &mut alpha).ok()?;
            let c = foreground.premultiply();
            let data = alpha.iter().flat_map(|&a| [c.r, c.g, c.b, c.a].map(|v| crate::paint::mul_div255(v, a))).collect();
            Pixmap { width: w, height: h, data }
        }
    };
    (pixels.width > 0 && pixels.height > 0).then_some((pixels, place))
}
