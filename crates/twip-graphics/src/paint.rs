//! How a fill is coloured: the brush (solid, linear or radial gradient, image pattern), its blend mode (source-over,
//! multiply, destination-in, clear), anti-aliasing, and colour filters (a 4×5 matrix or a 256-entry table).
//!
//! Colours enter as unpremultiplied 8-bit sRGB. A solid colour is premultiplied once, when it is recorded. Blending is
//! in encoded (non-linear) sRGB, 8 bits per channel: the arithmetic of Skia's untagged surfaces, which Twip drew with.

use crate::image::ImageId;

/// An unpremultiplied 8-bit sRGB colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha: 255 is opaque.
    pub a: u8,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
    /// Opaque black.
    pub const BLACK: Color = Color::rgb(0x00_0000);
    /// Opaque white.
    pub const WHITE: Color = Color::rgb(0xFF_FFFF);

    /// An opaque colour from `0xRRGGBB` (the form Twip's document model keeps colours in).
    pub const fn rgb(rgb: u32) -> Color {
        Color { r: (rgb >> 16) as u8, g: (rgb >> 8) as u8, b: rgb as u8, a: 255 }
    }

    /// A colour from its channels.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    /// This colour with alpha `a`.
    pub const fn with_alpha(self, a: u8) -> Color {
        Color { a, ..self }
    }

    /// With alpha `a` in `0.0..=1.0` (clamped), rounded to the nearest of the 256 steps.
    pub fn with_alpha_f(self, a: f32) -> Color {
        self.with_alpha((a.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    pub(crate) fn premultiply(self) -> PremulColor {
        let a = self.a;
        PremulColor { r: mul_div255(self.r, a), g: mul_div255(self.g, a), b: mul_div255(self.b, a), a }
    }
}

/// A premultiplied colour: each channel at most `a`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct PremulColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// `round(a · b / 255)`, exactly, without a division: the 8-bit product every backend uses (Skia's lowp `div255`).
pub(crate) const fn mul_div255(a: u8, b: u8) -> u8 {
    (((a as u32 * b as u32 + 128) * 257) >> 16) as u8
}

/// A gradient stop: `color` at `offset` along the gradient (`0.0` its start, `1.0` its end). Gradients interpolate
/// between stops in unpremultiplied colour, as Skia's do by default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    /// Where on the gradient (0.0 to 1.0) the colour is.
    pub offset: f32,
    /// The colour at that offset.
    pub color: Color,
}

/// What a gradient paints beyond its ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Extend {
    /// The end colours continue.
    #[default]
    Clamp,
    /// The gradient starts again.
    Repeat,
    /// The gradient runs back and forth.
    Mirror,
}

/// How an image's pixels are read when it is drawn at another size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sampling {
    /// The nearest pixel: for pattern tiles, which are pixel art.
    Nearest,
    /// Bilinear, from the mip level just larger than the drawn size: for pictures.
    #[default]
    Linear,
    /// The nearest pixel, a pattern's tile drawn a texel to each device pixel and placed on a pixel, under a transform
    /// without rotation or skew: its size in pixels is the same at any scale, as Word draws shading's stripes at any
    /// zoom. PDF draws it as nearest, at the transform's size.
    Pixel,
}

/// How a draw combines with what is under it, on premultiplied colour: the Porter-Duff operators, the separable blend
/// modes and the non-separable ones (W3C Compositing and Blending; the set COLRv1 composites with). Coverage lerps the
/// destination towards the result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// Erases the destination where the source covers it.
    Clear,
    /// The source, where it covers.
    Src,
    /// The destination, unchanged.
    Dst,
    #[default]
    /// The source over the destination.
    SrcOver,
    /// The destination over the source.
    DstOver,
    /// The source where the destination is opaque.
    SrcIn,
    /// Keeps the destination where the source is opaque: a soft-edge mask.
    DstIn,
    /// The source where the destination is transparent.
    SrcOut,
    /// The destination where the source is transparent.
    DstOut,
    /// The source over the destination, where the destination is opaque.
    SrcAtop,
    /// The destination over the source, where the source is opaque.
    DstAtop,
    /// Each where the other is transparent.
    Xor,
    /// The sum of source and destination.
    Plus,
    /// Inverse of the product of the inverses.
    Screen,
    /// Hard light with source and destination swapped.
    Overlay,
    /// The darker of the two.
    Darken,
    /// The lighter of the two.
    Lighten,
    /// Brightens the destination by the source.
    ColorDodge,
    /// Darkens the destination by the source.
    ColorBurn,
    /// Multiply or screen, by the source.
    HardLight,
    /// Darkens or lightens, by the source, softly.
    SoftLight,
    /// The absolute difference.
    Difference,
    /// Difference with lower contrast.
    Exclusion,
    /// Source times destination. The GPU backend draws it exactly in two fixed-function passes.
    Multiply,
    /// The source's hue with the destination's saturation and luminosity.
    Hue,
    /// The source's saturation with the destination's hue and luminosity.
    Saturation,
    /// The source's hue and saturation with the destination's luminosity.
    Color,
    /// The source's luminosity with the destination's hue and saturation.
    Luminosity,
    /// A GDI binary raster operation: each bit of the unpremultiplied 8-bit channels is `op`'s bit `2·s + d` (`s` the
    /// source's bit, `d` the destination's; the low four bits of `op` are read), composited as the separable modes
    /// are. 12 is the source, 10 the destination, 5 its inverse, 8 `and`, 14 `or`, 6 `xor`.
    Raster(u8),
}

impl BlendMode {
    /// The mode's index in declaration order, the raster operations from 28 by `op`: what the GPU shader switches on.
    #[cfg_attr(not(feature = "gpu"), allow(dead_code, reason = "only the GPU backend switches on it"))]
    pub(crate) fn index(self) -> u32 {
        match self {
            BlendMode::Raster(op) => 28 + u32::from(op & 15),
            BlendMode::Clear => 0,
            BlendMode::Src => 1,
            BlendMode::Dst => 2,
            BlendMode::SrcOver => 3,
            BlendMode::DstOver => 4,
            BlendMode::SrcIn => 5,
            BlendMode::DstIn => 6,
            BlendMode::SrcOut => 7,
            BlendMode::DstOut => 8,
            BlendMode::SrcAtop => 9,
            BlendMode::DstAtop => 10,
            BlendMode::Xor => 11,
            BlendMode::Plus => 12,
            BlendMode::Screen => 13,
            BlendMode::Overlay => 14,
            BlendMode::Darken => 15,
            BlendMode::Lighten => 16,
            BlendMode::ColorDodge => 17,
            BlendMode::ColorBurn => 18,
            BlendMode::HardLight => 19,
            BlendMode::SoftLight => 20,
            BlendMode::Difference => 21,
            BlendMode::Exclusion => 22,
            BlendMode::Multiply => 23,
            BlendMode::Hue => 24,
            BlendMode::Saturation => 25,
            BlendMode::Color => 26,
            BlendMode::Luminosity => 27,
        }
    }
}

/// `op`'s truth table applied to each bit of `s` and `d`: bit `2·s + d` of `op` (see [`BlendMode::Raster`]).
#[inline(always)]
pub(crate) fn raster_op(op: u8, s: u8, d: u8) -> u8 {
    let pick = |bit: u8, v: u8| if op & bit != 0 { v } else { 0 };
    pick(1, !s & !d) | pick(2, !s & d) | pick(4, s & !d) | pick(8, s & d)
}
/// The colour source of a fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Brush<'a> {
    /// A single colour.
    Solid(Color),
    /// From `start` to `end`, in the space `transform` maps into the draw's local space.
    Linear {
        /// Where the gradient starts.
        start: kurbo::Point,
        /// Where the gradient ends.
        end: kurbo::Point,
        /// The colour stops, in offset order.
        stops: &'a [Stop],
        /// What is painted outside the gradient's span.
        extend: Extend,
        /// Maps gradient space into the draw's local space.
        transform: kurbo::Affine,
    },
    /// Between two circles (a two-point conical gradient): `t` is 0 on the start circle and 1 on the end one, and a
    /// point takes the largest `t` whose circle passes through it with a radius of 0 or more; points no circle reaches
    /// are left alone. A plain radial gradient starts at its centre with radius 0. In the space `transform` maps into
    /// the draw's local space.
    Radial {
        /// The start circle's centre.
        start: kurbo::Point,
        /// The start circle's radius.
        start_radius: f64,
        /// The end circle's centre.
        end: kurbo::Point,
        /// The end circle's radius.
        end_radius: f64,
        /// The colour stops, in offset order.
        stops: &'a [Stop],
        /// What is painted outside the gradient's span.
        extend: Extend,
        /// Maps gradient space into the draw's local space.
        transform: kurbo::Affine,
    },
    /// Around `center`, from `start_angle` to `end_angle` in degrees, clockwise from the positive x axis (y is down);
    /// one turn is painted. In the space `transform` maps into the draw's local space.
    Sweep {
        /// The sweep's centre.
        center: kurbo::Point,
        /// The angle (degrees) at offset 0.
        start_angle: f64,
        /// The angle (degrees) at offset 1.
        end_angle: f64,
        /// The colour stops, in offset order.
        stops: &'a [Stop],
        /// What is painted outside the gradient's span.
        extend: Extend,
        /// Maps gradient space into the draw's local space.
        transform: kurbo::Affine,
    },
    /// An image repeated in both directions. `transform` maps image pixels into the draw's local space.
    Pattern {
        /// The image to repeat.
        image: ImageId,
        /// Maps image pixels into the draw's local space.
        transform: kurbo::Affine,
        /// How the image is sampled.
        sampling: Sampling,
    },
}

/// A fill's brush, blend mode and anti-aliasing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint<'a> {
    /// The colour source.
    pub brush: Brush<'a>,
    /// How the fill combines with what is under it.
    pub blend: BlendMode,
    /// Anti-aliased edges. Some of Twip's draws are aliased on purpose (the paper, caret bars), and frame comparisons
    /// depend on it.
    pub aa: bool,
}

impl<'a> Paint<'a> {
    /// `brush`, source-over, anti-aliased.
    pub const fn new(brush: Brush<'a>) -> Paint<'a> {
        Paint { brush, blend: BlendMode::SrcOver, aa: true }
    }
}

impl From<Color> for Paint<'_> {
    fn from(c: Color) -> Self {
        Paint::new(Brush::Solid(c))
    }
}

/// A per-pixel colour transform, applied to unpremultiplied colour in `0.0..=1.0`; each filter's result is clamped to
/// that range before the next filter in a chain sees it, as Skia's matrix filters clamp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColorFilter {
    /// Row-major 4×5: `r' = m[0]·r + m[1]·g + m[2]·b + m[3]·a + m[4]`, then the rows for g, b and a. The fifth column
    /// is in `0.0..=1.0` units, not 0–255.
    Matrix([f32; 20]),
    /// A lookup applied to each of r, g, b and a.
    Table([u8; 256]),
}

/// How an image is drawn: its sampling and colour filters (Twip's picture adjustments: transparency, grayscale,
/// brightness and contrast, duotone, bi-level), applied in order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImagePaint<'a> {
    /// How the image is sampled.
    pub sampling: Sampling,
    /// The filters applied to the image's colours, in order.
    pub color_filters: &'a [ColorFilter],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_div255_rounds_exactly() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                let exact = (a as f64 * b as f64 / 255.0).round() as u8;
                assert_eq!(mul_div255(a, b), exact, "{a} × {b}");
            }
        }
    }

    #[test]
    fn premultiply_scales_channels_by_alpha() {
        assert_eq!(Color::rgb(0xFF_8000).with_alpha(128).premultiply(), PremulColor { r: 128, g: 64, b: 0, a: 128 });
        assert_eq!(Color::rgb(0x12_3456).premultiply(), PremulColor { r: 0x12, g: 0x34, b: 0x56, a: 255 });
        assert_eq!(Color::WHITE.with_alpha_f(0.5).a, 128);
        assert_eq!(Color::WHITE.with_alpha_f(7.0).a, 255);
    }
}
