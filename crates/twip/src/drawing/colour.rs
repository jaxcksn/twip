//! DrawingML colours

use super::theme::{ColourMap, Theme};
use super::units::{Angle, Pct};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Rgba = Rgba { r: 255, g: 255, b: 255, a: 255 };

    pub fn rgb(v: u32) -> Rgba {
        Rgba { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8, a: 255 }
    }

    pub fn to_rgb(self) -> u32 {
        (u32::from(self.r) << 16) | (u32::from(self.g) << 8) | u32::from(self.b)
    }

    pub fn faded(self, alpha: f64) -> Rgba {
        Rgba { a: (f64::from(self.a) * alpha.clamp(0.0, 1.0)).round() as u8, ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SchemeSlot {
    Bg1,
    Tx1,
    Bg2,
    Tx2,
    Dk1,
    Lt1,
    Dk2,
    Lt2,
    Accent(u8),
    Hlink,
    FolHlink,
    Placeholder,
}

impl SchemeSlot {
    pub fn parse(s: &str) -> Option<SchemeSlot> {
        use SchemeSlot::*;
        Some(match s {
            "bg1" => Bg1,
            "tx1" => Tx1,
            "bg2" => Bg2,
            "tx2" => Tx2,
            "dk1" => Dk1,
            "lt1" => Lt1,
            "dk2" => Dk2,
            "lt2" => Lt2,
            "hlink" => Hlink,
            "folHlink" => FolHlink,
            "phClr" => Placeholder,
            _ => Accent(s.strip_prefix("accent")?.parse().ok().filter(|n| (1..=6).contains(n))?),
        })
    }

    pub fn name(self) -> String {
        use SchemeSlot::*;
        match self {
            Bg1 => "bg1".into(),
            Tx1 => "tx1".into(),
            Bg2 => "bg2".into(),
            Tx2 => "tx2".into(),
            Dk1 => "dk1".into(),
            Lt1 => "lt1".into(),
            Dk2 => "dk2".into(),
            Lt2 => "lt2".into(),
            Accent(n) => format!("accent{n}"),
            Hlink => "hlink".into(),
            FolHlink => "folHlink".into(),
            Placeholder => "phClr".into(),
        }
    }

    pub fn theme_index(self) -> Option<usize> {
        use SchemeSlot::*;
        Some(match self {
            Dk1 => 0,
            Lt1 => 1,
            Dk2 => 2,
            Lt2 => 3,
            Accent(n) => 3 + n as usize,
            Hlink => 10,
            FolHlink => 11,
            Bg1 | Tx1 | Bg2 | Tx2 | Placeholder => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ColourBase {
    Srgb(u32),
    ScRgb([Pct; 3]),
    Hsl { hue: Angle, sat: Pct, lum: Pct },
    Sys { name: String, last: u32 },
    Preset { name: String, value: u32 },
    Scheme(SchemeSlot),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColourMod {
    Tint(Pct),
    Shade(Pct),
    Comp,
    Inv,
    Gray,
    Alpha(Pct),
    AlphaOff(Pct),
    AlphaMod(Pct),
    Hue(Angle),
    HueOff(Angle),
    HueMod(Pct),
    Sat(Pct),
    SatOff(Pct),
    SatMod(Pct),
    Lum(Pct),
    LumOff(Pct),
    LumMod(Pct),
    Red(Pct),
    RedOff(Pct),
    RedMod(Pct),
    Green(Pct),
    GreenOff(Pct),
    GreenMod(Pct),
    Blue(Pct),
    BlueOff(Pct),
    BlueMod(Pct),
    Gamma,
    InvGamma,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Colour {
    pub base: ColourBase,
    pub mods: Vec<ColourMod>,
}

impl Colour {
    pub fn srgb(v: u32) -> Colour {
        Colour { base: ColourBase::Srgb(v), mods: Vec::new() }
    }

    pub fn scheme(slot: SchemeSlot) -> Colour {
        Colour { base: ColourBase::Scheme(slot), mods: Vec::new() }
    }

    pub fn with(mut self, m: ColourMod) -> Colour {
        self.mods.push(m);
        self
    }

    pub fn resolve(&self, theme: &Theme, map: &ColourMap, placeholder: Option<Rgba>) -> Rgba {
        let base = match &self.base {
            ColourBase::Srgb(v) | ColourBase::Sys { last: v, .. } | ColourBase::Preset { value: v, .. } => Rgba::rgb(*v),
            ColourBase::ScRgb([r, g, b]) => {
                let c = |p: &Pct| (linear_to_srgb(p.fraction().clamp(0.0, 1.0)) * 255.0).round() as u8;
                Rgba { r: c(r), g: c(g), b: c(b), a: 255 }
            }
            ColourBase::Hsl { hue, sat, lum } => {
                let (r, g, b) = hsl_to_rgb(hue.degrees().rem_euclid(360.0), sat.fraction().clamp(0.0, 1.0), lum.fraction().clamp(0.0, 1.0));
                Rgba { r: unit(r), g: unit(g), b: unit(b), a: 255 }
            }
            ColourBase::Scheme(SchemeSlot::Placeholder) => placeholder.unwrap_or(Rgba::BLACK),
            ColourBase::Scheme(slot) => theme.colour(map.map(*slot)),
        };
        apply(base, &self.mods)
    }
}

fn unit(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A two-stop gradient's blend at sixteenths of the way: the share of the second colour, in linear light.
pub const TWO_STOP_BLEND: [f64; 17] =
    [0.0, 0.018, 0.0472, 0.0843, 0.1404, 0.209, 0.2927, 0.3873, 0.4934, 0.5922, 0.6913, 0.778, 0.8485, 0.9054, 0.949, 0.9779, 1.0];

pub fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// Word's `darken(n)` of a one-colour gradient: each channel scaled by `n`/255.
pub fn darken(c: u32, n: u8) -> u32 {
    [16, 8, 0].iter().map(|s| ((((c >> s) & 0xFF) * u32::from(n) + 127) / 255) << s).sum()
}

/// Word's `lighten(n)` of a one-colour gradient: each channel's distance from white scaled by `n`/255.
pub fn lighten(c: u32, n: u8) -> u32 {
    0xFF_FFFF - darken(0xFF_FFFF - c, n)
}

/// The `darken(n)` (`false`) or `lighten(n)` (`true`) that makes `to` of `from`, when one does.
pub fn shade_of(from: u32, to: u32) -> Option<(bool, u8)> {
    (0..=255u8).rev().find_map(|n| {
        if darken(from, n) == to {
            Some((false, n))
        } else if lighten(from, n) == to {
            Some((true, n))
        } else {
            None
        }
    })
}

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |t: f64| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let h = h / 360.0;
    (f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0))
}

fn apply(base: Rgba, mods: &[ColourMod]) -> Rgba {
    use ColourMod::*;
    let mut c = [f64::from(base.r) / 255.0, f64::from(base.g) / 255.0, f64::from(base.b) / 255.0];
    let mut a = f64::from(base.a) / 255.0;
    let hsl = |c: &[f64; 3], f: &dyn Fn(f64, f64, f64) -> (f64, f64, f64)| {
        let (h, s, l) = rgb_to_hsl(c[0], c[1], c[2]);
        let (h, s, l) = f(h, s, l);
        let (r, g, b) = hsl_to_rgb(h.rem_euclid(360.0), s.max(0.0), l);
        [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)]
    };
    let linear = |c: &[f64; 3], f: &dyn Fn([f64; 3]) -> [f64; 3]| {
        let l = f([srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2])]);
        [linear_to_srgb(l[0].clamp(0.0, 1.0)), linear_to_srgb(l[1].clamp(0.0, 1.0)), linear_to_srgb(l[2].clamp(0.0, 1.0))]
    };
    let channel = |c: &[f64; 3], i: usize, f: &dyn Fn(f64) -> f64| {
        linear(c, &|mut l| {
            l[i] = f(l[i]);
            l
        })
    };
    for m in mods {
        c = match *m {
            Tint(p) => linear(&c, &|l| l.map(|v| 1.0 - (1.0 - v) * p.fraction())),
            Shade(p) => linear(&c, &|l| l.map(|v| v * p.fraction())),
            Comp => hsl(&c, &|h, s, l| (h + 180.0, s, l)),
            Inv => linear(&c, &|l| l.map(|v| 1.0 - v)),
            Gray => {
                let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                [y, y, y]
            }
            Alpha(p) => {
                a = p.fraction().clamp(0.0, 1.0);
                c
            }
            AlphaOff(p) => {
                a = (a + p.fraction()).clamp(0.0, 1.0);
                c
            }
            AlphaMod(p) => {
                a = (a * p.fraction()).clamp(0.0, 1.0);
                c
            }
            Hue(v) => hsl(&c, &|_, s, l| (v.degrees(), s, l)),
            HueOff(v) => hsl(&c, &|h, s, l| (h + v.degrees(), s, l)),
            HueMod(p) => hsl(&c, &|h, s, l| (h * p.fraction(), s, l)),
            Sat(p) => hsl(&c, &|h, _, l| (h, p.fraction(), l)),
            SatOff(p) => hsl(&c, &|h, s, l| (h, s + p.fraction(), l)),
            SatMod(p) => hsl(&c, &|h, s, l| (h, s * p.fraction(), l)),
            Lum(p) => hsl(&c, &|h, s, _| (h, s, p.fraction())),
            LumOff(p) => hsl(&c, &|h, s, l| (h, s, l + p.fraction())),
            LumMod(p) => hsl(&c, &|h, s, l| (h, s, l * p.fraction())),
            Red(p) => channel(&c, 0, &|_| p.fraction()),
            RedOff(p) => channel(&c, 0, &|v| v + p.fraction()),
            RedMod(p) => channel(&c, 0, &|v| v * p.fraction()),
            Green(p) => channel(&c, 1, &|_| p.fraction()),
            GreenOff(p) => channel(&c, 1, &|v| v + p.fraction()),
            GreenMod(p) => channel(&c, 1, &|v| v * p.fraction()),
            Blue(p) => channel(&c, 2, &|_| p.fraction()),
            BlueOff(p) => channel(&c, 2, &|v| v + p.fraction()),
            BlueMod(p) => channel(&c, 2, &|v| v * p.fraction()),
            Gamma => c.map(|v| linear_to_srgb(v.clamp(0.0, 1.0))),
            InvGamma => c.map(|v| srgb_to_linear(v.clamp(0.0, 1.0))),
        };
    }
    Rgba { r: unit(c[0]), g: unit(c[1]), b: unit(c[2]), a: unit(a) }
}

pub fn preset_colour(name: &str) -> Option<u32> {
    let full = match name {
        n if n.starts_with("dk") => format!("dark{}", &n[2..]),
        n if n.starts_with("lt") => format!("light{}", &n[2..]),
        n if n.starts_with("med") && !n.starts_with("medium") => format!("medium{}", &n[3..]),
        n => n.to_string(),
    };
    let key = full.to_ascii_lowercase();
    CSS.binary_search_by(|(n, _)| n.cmp(&key.as_str())).ok().map(|i| CSS[i].1)
}

const CSS: &[(&str, u32)] = &[
    ("aliceblue", 0xF0F8FF),
    ("antiquewhite", 0xFAEBD7),
    ("aqua", 0x00FFFF),
    ("aquamarine", 0x7FFFD4),
    ("azure", 0xF0FFFF),
    ("beige", 0xF5F5DC),
    ("bisque", 0xFFE4C4),
    ("black", 0x000000),
    ("blanchedalmond", 0xFFEBCD),
    ("blue", 0x0000FF),
    ("blueviolet", 0x8A2BE2),
    ("brown", 0xA52A2A),
    ("burlywood", 0xDEB887),
    ("cadetblue", 0x5F9EA0),
    ("chartreuse", 0x7FFF00),
    ("chocolate", 0xD2691E),
    ("coral", 0xFF7F50),
    ("cornflowerblue", 0x6495ED),
    ("cornsilk", 0xFFF8DC),
    ("crimson", 0xDC143C),
    ("cyan", 0x00FFFF),
    ("darkblue", 0x00008B),
    ("darkcyan", 0x008B8B),
    ("darkgoldenrod", 0xB8860B),
    ("darkgray", 0xA9A9A9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xA9A9A9),
    ("darkkhaki", 0xBDB76B),
    ("darkmagenta", 0x8B008B),
    ("darkolivegreen", 0x556B2F),
    ("darkorange", 0xFF8C00),
    ("darkorchid", 0x9932CC),
    ("darkred", 0x8B0000),
    ("darksalmon", 0xE9967A),
    ("darkseagreen", 0x8FBC8F),
    ("darkslateblue", 0x483D8B),
    ("darkslategray", 0x2F4F4F),
    ("darkslategrey", 0x2F4F4F),
    ("darkturquoise", 0x00CED1),
    ("darkviolet", 0x9400D3),
    ("deeppink", 0xFF1493),
    ("deepskyblue", 0x00BFFF),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1E90FF),
    ("firebrick", 0xB22222),
    ("floralwhite", 0xFFFAF0),
    ("forestgreen", 0x228B22),
    ("fuchsia", 0xFF00FF),
    ("gainsboro", 0xDCDCDC),
    ("ghostwhite", 0xF8F8FF),
    ("gold", 0xFFD700),
    ("goldenrod", 0xDAA520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xADFF2F),
    ("grey", 0x808080),
    ("honeydew", 0xF0FFF0),
    ("hotpink", 0xFF69B4),
    ("indianred", 0xCD5C5C),
    ("indigo", 0x4B0082),
    ("ivory", 0xFFFFF0),
    ("khaki", 0xF0E68C),
    ("lavender", 0xE6E6FA),
    ("lavenderblush", 0xFFF0F5),
    ("lawngreen", 0x7CFC00),
    ("lemonchiffon", 0xFFFACD),
    ("lightblue", 0xADD8E6),
    ("lightcoral", 0xF08080),
    ("lightcyan", 0xE0FFFF),
    ("lightgoldenrodyellow", 0xFAFAD2),
    ("lightgray", 0xD3D3D3),
    ("lightgreen", 0x90EE90),
    ("lightgrey", 0xD3D3D3),
    ("lightpink", 0xFFB6C1),
    ("lightsalmon", 0xFFA07A),
    ("lightseagreen", 0x20B2AA),
    ("lightskyblue", 0x87CEFA),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xB0C4DE),
    ("lightyellow", 0xFFFFE0),
    ("lime", 0x00FF00),
    ("limegreen", 0x32CD32),
    ("linen", 0xFAF0E6),
    ("magenta", 0xFF00FF),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66CDAA),
    ("mediumblue", 0x0000CD),
    ("mediumorchid", 0xBA55D3),
    ("mediumpurple", 0x9370DB),
    ("mediumseagreen", 0x3CB371),
    ("mediumslateblue", 0x7B68EE),
    ("mediumspringgreen", 0x00FA9A),
    ("mediumturquoise", 0x48D1CC),
    ("mediumvioletred", 0xC71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xF5FFFA),
    ("mistyrose", 0xFFE4E1),
    ("moccasin", 0xFFE4B5),
    ("navajowhite", 0xFFDEAD),
    ("navy", 0x000080),
    ("oldlace", 0xFDF5E6),
    ("olive", 0x808000),
    ("olivedrab", 0x6B8E23),
    ("orange", 0xFFA500),
    ("orangered", 0xFF4500),
    ("orchid", 0xDA70D6),
    ("palegoldenrod", 0xEEE8AA),
    ("palegreen", 0x98FB98),
    ("paleturquoise", 0xAFEEEE),
    ("palevioletred", 0xDB7093),
    ("papayawhip", 0xFFEFD5),
    ("peachpuff", 0xFFDAB9),
    ("peru", 0xCD853F),
    ("pink", 0xFFC0CB),
    ("plum", 0xDDA0DD),
    ("powderblue", 0xB0E0E6),
    ("purple", 0x800080),
    ("red", 0xFF0000),
    ("rosybrown", 0xBC8F8F),
    ("royalblue", 0x4169E1),
    ("saddlebrown", 0x8B4513),
    ("salmon", 0xFA8072),
    ("sandybrown", 0xF4A460),
    ("seagreen", 0x2E8B57),
    ("seashell", 0xFFF5EE),
    ("sienna", 0xA0522D),
    ("silver", 0xC0C0C0),
    ("skyblue", 0x87CEEB),
    ("slateblue", 0x6A5ACD),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xFFFAFA),
    ("springgreen", 0x00FF7F),
    ("steelblue", 0x4682B4),
    ("tan", 0xD2B48C),
    ("teal", 0x008080),
    ("thistle", 0xD8BFD8),
    ("tomato", 0xFF6347),
    ("turquoise", 0x40E0D0),
    ("violet", 0xEE82EE),
    ("wheat", 0xF5DEB3),
    ("white", 0xFFFFFF),
    ("whitesmoke", 0xF5F5F5),
    ("yellow", 0xFFFF00),
    ("yellowgreen", 0x9ACD32),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preset_colour_table_is_sorted_and_reads_abbreviations() {
        assert!(CSS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(preset_colour("dkBlue"), Some(0x00008B));
        assert_eq!(preset_colour("medPurple"), Some(0x9370DB));
        assert_eq!(preset_colour("ltGray"), Some(0xD3D3D3));
        assert_eq!(preset_colour("coral"), Some(0xFF7F50));
        assert_eq!(preset_colour("nope"), None);
    }
}
