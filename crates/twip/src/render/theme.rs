//! The editor chrome's colours: a light or dark scheme's tokens (the desk, the paper-side surfaces, three inks and
//! lines) and an accent the host chooses. Everything the engine draws around and over the document takes its
//! colours from here; the document's own colours are [`super::appearance`]'s.

pub use twip_graphics::paint::Color;

/// A colour from its alpha, red, green and blue.
pub const fn argb(a: u8, r: u8, g: u8, b: u8) -> Color {
    Color::rgba(r, g, b, a)
}

/// An opaque colour from its red, green and blue.
pub const fn rgb8(r: u8, g: u8, b: u8) -> Color {
    Color::rgba(r, g, b, 255)
}

/// Colours the platform supplies in high contrast mode (the host reads them; the engine never asks the platform).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SystemColours {
    pub canvas: Color,
    pub text: Color,
    pub highlight: Color,
    pub highlight_text: Color,
    pub gray_text: Color,
}

/// The chrome's scheme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scheme {
    Light,
    Dark,
    HighContrast(SystemColours),
}

/// One scheme's colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// Around the pages.
    pub desk: Color,
    /// Chrome surfaces next to the page: the rulers' strip, and chips' and labels' backgrounds. What's drawn on the
    /// page (handles, the options button) takes the page's colour instead.
    pub paper: Color,
    /// Text and markers, strongest to faintest.
    pub ink: Color,
    pub ink2: Color,
    pub ink3: Color,
    /// Hairlines and hatching (with alpha).
    pub line: Color,
    /// How dark the page shadow is: a factor on the light scheme's alphas.
    pub shadow: f32,
}

impl Tokens {
    pub fn of(scheme: Scheme) -> Tokens {
        match scheme {
            Scheme::Light => Tokens {
                desk: rgb(0xe8e7e3),
                paper: rgb(0xffffff),
                ink: rgb(0x1c1d1f),
                ink2: rgb(0x62656c),
                ink3: rgb(0xa4a6ac),
                line: argb(31, 20, 22, 26),
                shadow: 1.0,
            },
            Scheme::Dark => Tokens {
                desk: rgb(0x111214),
                paper: rgb(0x222327),
                ink: rgb(0xe7e7ea),
                ink2: rgb(0xa0a3aa),
                ink3: rgb(0x5f626a),
                line: argb(31, 255, 255, 255),
                shadow: 4.0,
            },
            Scheme::HighContrast(s) => {
                Tokens { desk: s.canvas, paper: s.canvas, ink: s.text, ink2: s.text, ink3: s.gray_text, line: s.text, shadow: 0.0 }
            }
        }
    }
}

/// The accent and what's derived from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Accent {
    /// The host's colour.
    pub base: Color,
    /// What's drawn: the base in the light scheme; in the dark one, lighter and less saturated so it holds against
    /// dark surfaces (OKLCH lightness 0.72, chroma at most 0.15).
    pub solid: Color,
    /// `solid` at 14% and 40%: hover tints and previews, and the selection.
    pub soft: Color,
    pub mid: Color,
    /// Text and glyphs drawn on `solid`.
    pub on: Color,
}

/// The default accent, azure: OKLCH(0.57, 0.19, 256).
pub const AZURE: Color = Color::rgb(0x0a_74e4);

impl Accent {
    pub fn new(base: Color, scheme: Scheme) -> Accent {
        let solid = match scheme {
            Scheme::Light => base,
            Scheme::Dark => {
                let (_, c, h) = oklch(base);
                from_oklch(0.72, c.min(0.15), h)
            }
            Scheme::HighContrast(s) => s.highlight,
        };
        let on = match scheme {
            Scheme::HighContrast(s) => s.highlight_text,
            _ => {
                // The text colour with the higher contrast on the accent; black when neither of the two reaches 4.5:1.
                let best = [Color::WHITE, rgb(0x0c1220)].into_iter().max_by(|a, b| contrast(*a, solid).total_cmp(&contrast(*b, solid)));
                best.filter(|c| contrast(*c, solid) >= 4.5).unwrap_or(Color::BLACK)
            }
        };
        Accent { base, solid, soft: solid.with_alpha(36), mid: solid.with_alpha(102), on }
    }
}

/// The chrome's scheme, tokens and accent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub scheme: Scheme,
    pub tokens: Tokens,
    pub accent: Accent,
}

impl Default for Theme {
    fn default() -> Theme {
        Theme::new(Scheme::Light, AZURE)
    }
}

impl Theme {
    pub fn new(scheme: Scheme, accent: Color) -> Theme {
        Theme { scheme, tokens: Tokens::of(scheme), accent: Accent::new(accent, scheme) }
    }

    pub fn is_dark(&self) -> bool {
        match self.scheme {
            Scheme::Light => false,
            Scheme::Dark => true,
            Scheme::HighContrast(s) => luminance(s.canvas) < 0.2,
        }
    }

    /// The overlay colours the theme gives.
    pub fn palette(&self) -> super::Palette {
        let (t, a) = (&self.tokens, &self.accent);
        super::Palette {
            // Light: an opaque tint multiplied under the content, so text keeps its colour; dark: the accent over it.
            selection: if self.is_dark() { a.mid } else { mix(Color::WHITE, a.solid, 0.4) },
            object: a.solid,
            drop: t.ink2,
            marks: a.solid,
            leaders: t.ink2,
        }
    }
}

pub fn rgb(c: u32) -> Color {
    Color::rgb(c)
}

/// `a` and `b` mixed: `t` of `b`.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    rgb8(m(a.r, b.r), m(a.g, b.g), m(a.b, b.b))
}

fn linear(v: u8) -> f64 {
    let v = f64::from(v) / 255.0;
    if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn gamma(v: f64) -> u8 {
    let v = if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.max(0.0).powf(1.0 / 2.4) - 0.055 };
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// WCAG relative luminance.
pub fn luminance(c: Color) -> f64 {
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

/// WCAG contrast ratio.
pub fn contrast(a: Color, b: Color) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// OKLCH: lightness, chroma, hue in degrees.
fn oklch(c: Color) -> (f64, f64, f64) {
    let (r, g, b) = (linear(c.r), linear(c.g), linear(c.b));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let ll = 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let bb = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    (ll, a.hypot(bb), bb.atan2(a).to_degrees())
}

fn from_oklch(l: f64, c: f64, h: f64) -> Color {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m_ = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s_ = (l - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    rgb8(
        gamma(4.076_741_662_1 * l_ - 3.307_711_591_3 * m_ + 0.230_969_929_2 * s_),
        gamma(-1.268_438_004_6 * l_ + 2.609_757_401_1 * m_ - 0.341_319_396_5 * s_),
        gamma(-0.004_196_086_3 * l_ - 0.703_418_614_7 * m_ + 1.707_614_701 * s_),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dark_azure_is_the_prototypes() {
        let a = Accent::new(AZURE, Scheme::Dark).solid;
        let want = rgb(0x61a6ff);
        for (x, y) in [(a.r, want.r), (a.g, want.g), (a.b, want.b)] {
            assert!(x.abs_diff(y) <= 2, "{a:?} vs {want:?}");
        }
    }

    #[test]
    fn text_on_the_accent_reaches_4_5_to_1_for_every_hue() {
        for h in (0..360).step_by(10) {
            for (l, c) in [(0.45, 0.15), (0.57, 0.19), (0.7, 0.12)] {
                for scheme in [Scheme::Light, Scheme::Dark] {
                    let a = Accent::new(from_oklch(l, c, f64::from(h)), scheme);
                    let r = contrast(a.on, a.solid);
                    assert!(r >= 4.5, "hue {h} L {l} {scheme:?}: {r:.2}");
                }
            }
        }
    }

    #[test]
    fn text_over_the_selection_keeps_4_5_to_1_in_both_schemes() {
        // Light: black text over the tint multiplied into a white page. Dark: the dark page's text (F6F6F6) over the
        // accent at 40% laid over the dark page (262626).
        let light = Theme::new(Scheme::Light, AZURE).palette().selection;
        assert!(contrast(Color::BLACK, light) >= 4.5);
        for h in (0..360).step_by(30) {
            let dark = Theme::new(Scheme::Dark, from_oklch(0.57, 0.19, f64::from(h))).palette().selection;
            let over = mix(rgb(0x262626), dark.with_alpha(255), f32::from(dark.a) / 255.0);
            let r = contrast(rgb(0xF6F6F6), over);
            assert!(r >= 4.5, "hue {h}: {r:.2}");
        }
    }

    #[test]
    fn markers_and_handles_hold_3_to_1_with_the_three_accents_in_both_schemes() {
        // Azure (the default), cobalt and cerulean: the prototype's accents, hues 256, 268 and 238.
        for hue in [256.0, 268.0, 238.0] {
            for scheme in [Scheme::Light, Scheme::Dark] {
                let t = Theme::new(scheme, from_oklch(0.57, 0.19, hue));
                let r = contrast(t.accent.solid, t.tokens.paper);
                assert!(r >= 3.0, "hue {hue} {scheme:?}: accent on the chrome {r:.2}");
                assert!(contrast(t.tokens.ink2, t.tokens.paper) >= 3.0, "{scheme:?}: ink2 markers");
                assert!(contrast(t.accent.on, t.accent.solid) >= 4.5, "hue {hue} {scheme:?}: text on the accent");
            }
        }
    }

    #[test]
    fn chrome_text_holds_on_the_chrome_in_both_schemes() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let t = Tokens::of(scheme);
            assert!(contrast(t.ink, t.paper) >= 4.5, "{scheme:?} ink");
            assert!(contrast(t.ink2, t.paper) >= 4.5, "{scheme:?} ink2");
            assert!(contrast(t.ink3, t.paper) >= 2.0, "{scheme:?} ink3");
            let a = Accent::new(AZURE, scheme);
            assert!(contrast(a.solid, t.paper) >= 3.0, "{scheme:?} accent on paper");
        }
    }
}
