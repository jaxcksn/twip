//! How a fill is coloured: the brush (solid, linear or radial gradient, image pattern), its blend mode (source-over,
//! multiply, destination-in, clear), anti-aliasing, and colour filters (a 4×5 matrix or a 256-entry table).
//!
//! Colours enter as unpremultiplied 8-bit sRGB. A solid colour is premultiplied once, when it is recorded. Blending is
//! in encoded (non-linear) sRGB, 8 bits per channel: the arithmetic of Skia's untagged surfaces, which Twip drew with.

use crate::image::ImageId;

/// An unpremultiplied 8-bit sRGB colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
    pub const BLACK: Color = Color::rgb(0x00_0000);
    pub const WHITE: Color = Color::rgb(0xFF_FFFF);

    /// An opaque colour from `0xRRGGBB` (the form Twip's document model keeps colours in).
    pub const fn rgb(rgb: u32) -> Color {
        Color { r: (rgb >> 16) as u8, g: (rgb >> 8) as u8, b: rgb as u8, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

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
    pub offset: f32,
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
}

/// How a draw combines with what is under it, on premultiplied colour: the Porter-Duff operators, the separable blend
/// modes and the non-separable ones (W3C Compositing and Blending; the set COLRv1 composites with). Coverage lerps the
/// destination towards the result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// Erases the destination where the source covers it.
    Clear,
    Src,
    Dst,
    #[default]
    SrcOver,
    DstOver,
    SrcIn,
    /// Keeps the destination where the source is opaque: a soft-edge mask.
    DstIn,
    SrcOut,
    DstOut,
    SrcAtop,
    DstAtop,
    Xor,
    Plus,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    /// Source times destination. The GPU backend draws it exactly in two fixed-function passes.
    Multiply,
    Hue,
    Saturation,
    Color,
    Luminosity,
}
/// The colour source of a fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Brush<'a> {
    Solid(Color),
    /// From `start` to `end`, in the space `transform` maps into the draw's local space.
    Linear {
        start: kurbo::Point,
        end: kurbo::Point,
        stops: &'a [Stop],
        extend: Extend,
        transform: kurbo::Affine,
    },
    /// Between two circles (a two-point conical gradient): `t` is 0 on the start circle and 1 on the end one, and a
    /// point takes the largest `t` whose circle passes through it with a radius of 0 or more; points no circle reaches
    /// are left alone. A plain radial gradient starts at its centre with radius 0. In the space `transform` maps into
    /// the draw's local space.
    Radial {
        start: kurbo::Point,
        start_radius: f64,
        end: kurbo::Point,
        end_radius: f64,
        stops: &'a [Stop],
        extend: Extend,
        transform: kurbo::Affine,
    },
    /// Around `center`, from `start_angle` to `end_angle` in degrees, clockwise from the positive x axis (y is down);
    /// one turn is painted. In the space `transform` maps into the draw's local space.
    Sweep {
        center: kurbo::Point,
        start_angle: f64,
        end_angle: f64,
        stops: &'a [Stop],
        extend: Extend,
        transform: kurbo::Affine,
    },
    /// An image repeated in both directions. `transform` maps image pixels into the draw's local space.
    Pattern {
        image: ImageId,
        transform: kurbo::Affine,
        sampling: Sampling,
    },
}

/// A fill's brush, blend mode and anti-aliasing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint<'a> {
    pub brush: Brush<'a>,
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
    pub sampling: Sampling,
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
