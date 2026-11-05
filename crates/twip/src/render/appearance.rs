//! How the document's colours are drawn: in their own colours, or on a dark page (DRAW-27). Only screen drawing reads
//! this; layout, the document, PDF export and printing never see it.

use std::sync::OnceLock;

/// The page's appearance on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Appearance {
    #[default]
    Light,
    /// A dark page; `page: Light` draws the page in its own colours while the chrome stays dark.
    Dark { page: PageMode },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageMode {
    Dark,
    Light,
}

impl Appearance {
    /// Whether colours are drawn as they are.
    pub fn is_identity(self) -> bool {
        !matches!(self, Appearance::Dark { page: PageMode::Dark })
    }

    /// The RGB colour `c` (0xRRGGBB) is drawn in.
    pub fn map(self, c: u32) -> u32 {
        if self.is_identity() { c } else { dark(c) }
    }
}

const LEVELS: usize = 17;

/// The dark-page table (DRAW-27): 17 levels per channel, `[r][g][b]` flattened.
fn table() -> &'static [[u8; 3]] {
    static TABLE: OnceLock<Vec<[u8; 3]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let text = include_str!("dark-page.txt");
        let hex = |s: &str| u32::from_str_radix(s, 16).map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8]);
        let t: Vec<[u8; 3]> = text.lines().filter(|l| !l.starts_with('#')).flat_map(|l| l.split(' ')).filter_map(|s| hex(s).ok()).collect();
        debug_assert_eq!(t.len(), LEVELS * LEVELS * LEVELS, "the dark-page table has 17^3 entries");
        t
    })
}

/// `c` on a dark page: interpolated in the table between the 4 corners of the tetrahedron it lies in, but for the
/// pure blues between 0x80 and 0x90, which step at 0x8E (DRAW-27).
// shortcut: other saturated blues and violets (red and green under about 0x80, blue over about 0xA0) also sit near a
// step the 17 levels don't follow: on a 600-colour random sample 3% are off by more than CIEDE2000 3 (up to 15), the
// rest by 0.4 on average. Following them needs the mapping itself, not a finer table (steps of 8 still miss).
fn dark(c: u32) -> u32 {
    if c >> 8 == 0 && (0x80..0x90).contains(&c) {
        let (lo, hi) = if c < 0x8E { ((0x80, 0xD0D0FF), (0x8D, 0xCBCBFF)) } else { ((0x8E, 0x49D1FF), (0x90, 0x48D1FF)) };
        let f = (c - lo.0) as f64 / (hi.0 - lo.0) as f64;
        let ch = |s: u32| {
            let (a, b) = (((lo.1 >> s) & 0xFF) as f64, ((hi.1 >> s) & 0xFF) as f64);
            ((a + (b - a) * f).round() as u32) << s
        };
        return ch(16) | ch(8) | ch(0);
    }
    let t = table();
    if t.len() != LEVELS * LEVELS * LEVELS {
        return c;
    }
    let ch = [(c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF].map(|v| v as f64);
    let step = 255.0 / (LEVELS - 1) as f64;
    let i = ch.map(|v| ((v / step) as usize).min(LEVELS - 2));
    let level = |k: usize| (k as f64 * step).round();
    let f = [0, 1, 2].map(|k| (ch[k] - level(i[k])) / (level(i[k] + 1) - level(i[k])));
    let at = |d: [usize; 3]| t[((i[0] + d[0]) * LEVELS + i[1] + d[1]) * LEVELS + i[2] + d[2]].map(f64::from);
    let mut order = [0usize, 1, 2];
    order.sort_by(|a, b| f[*b].total_cmp(&f[*a]));
    let mut d = [0usize; 3];
    let mut prev = at(d);
    let mut out = prev;
    for k in order {
        d[k] = 1;
        let cur = at(d);
        for n in 0..3 {
            out[n] += f[k] * (cur[n] - prev[n]);
        }
        prev = cur;
    }
    let b = out.map(|v| v.round().clamp(0.0, 255.0) as u32);
    (b[0] << 16) | (b[1] << 8) | b[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    const DARK: Appearance = Appearance::Dark { page: PageMode::Dark };

    /// CIEDE2000 between two sRGB colours.
    fn de2000(a: u32, b: u32) -> f64 {
        let lab = |c: u32| {
            let lin = |v: u32| {
                let v = f64::from(v) / 255.0;
                if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            let (r, g, b) = (lin((c >> 16) & 0xFF), lin((c >> 8) & 0xFF), lin(c & 0xFF));
            let x = (0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b) / 0.950_47;
            let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
            let z = (0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b) / 1.088_83;
            let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
            let (fx, fy, fz) = (f(x), f(y), f(z));
            (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
        };
        let ((l1, a1, b1), (l2, a2, b2)) = (lab(a), lab(b));
        let cm = (a1.hypot(b1) + a2.hypot(b2)) / 2.0;
        let g = 0.5 * (1.0 - (cm.powi(7) / (cm.powi(7) + 25f64.powi(7))).sqrt());
        let (a1p, a2p) = (a1 * (1.0 + g), a2 * (1.0 + g));
        let (c1, c2) = (a1p.hypot(b1), a2p.hypot(b2));
        let hue = |b: f64, a: f64| b.atan2(a).to_degrees().rem_euclid(360.0);
        let (h1, h2) = (hue(b1, a1p), hue(b2, a2p));
        let dh = if c1 * c2 == 0.0 {
            0.0
        } else if (h2 - h1).abs() <= 180.0 {
            h2 - h1
        } else if h2 > h1 {
            h2 - h1 - 360.0
        } else {
            h2 - h1 + 360.0
        };
        let dhh = 2.0 * (c1 * c2).sqrt() * (dh / 2.0).to_radians().sin();
        let (lm, cpm) = ((l1 + l2) / 2.0, (c1 + c2) / 2.0);
        let hm = if c1 * c2 == 0.0 {
            h1 + h2
        } else if (h1 - h2).abs() <= 180.0 {
            (h1 + h2) / 2.0
        } else if h1 + h2 < 360.0 {
            (h1 + h2 + 360.0) / 2.0
        } else {
            (h1 + h2 - 360.0) / 2.0
        };
        let t = 1.0 - 0.17 * (hm - 30.0).to_radians().cos() + 0.24 * (2.0 * hm).to_radians().cos() + 0.32 * (3.0 * hm + 6.0).to_radians().cos()
            - 0.20 * (4.0 * hm - 63.0).to_radians().cos();
        let dth = 30.0 * (-((hm - 275.0) / 25.0).powi(2)).exp();
        let rc = 2.0 * (cpm.powi(7) / (cpm.powi(7) + 25f64.powi(7))).sqrt();
        let sl = 1.0 + 0.015 * (lm - 50.0).powi(2) / (20.0 + (lm - 50.0).powi(2)).sqrt();
        let (sc, sh) = (1.0 + 0.045 * cpm, 1.0 + 0.015 * cpm * t);
        let rt = -(2.0 * dth).to_radians().sin() * rc;
        let (x, y, z) = ((l2 - l1) / sl, (c2 - c1) / sc, dhh / sh);
        (x * x + y * y + z * z + rt * y * z).sqrt()
    }

    /// Colours off the table's points, and what a dark page draws them in: text, shading, borders, the page colour,
    /// a hyperlink and a revision mark.
    const PAIRS: [(u32, u32); 24] = [
        (0x370606, 0xFFDADA),
        (0x063706, 0xC3DEC3),
        (0x060637, 0xE5E5FF),
        (0x6E0C0C, 0xFFBDBD),
        (0x0C6E0C, 0x88C588),
        (0x0C0C6E, 0xD4D4FF),
        (0xA51212, 0xFF9A9A),
        (0x12A512, 0x1EA81E),
        (0x1212A5, 0x30C8FF),
        (0xDC1818, 0xFF7070),
        (0x18DC18, 0x008700),
        (0x1818DC, 0x00B4FF),
        (0xEB4747, 0xFF6565),
        (0x47EB47, 0x007900),
        (0x4747EB, 0x00A7FF),
        (0xF17E7E, 0xD25D5D),
        (0x7EF17E, 0x006900),
        (0xF7B6B6, 0x914C4C),
        (0xE3E3FC, 0x40405B),
        (0x929292, 0x8E8E8E),
        (0xDBDBDB, 0x4C4C4C),
        (0xFFF2CC, 0x433300),
        (0x0563C1, 0x7EB1FF),
        (0xD13438, 0xFF7A7C),
    ];

    #[test]
    fn colours_on_a_dark_page_follow_the_table() {
        for (from, want) in PAIRS {
            let got = DARK.map(from);
            let e = de2000(got, want);
            assert!(e <= 3.0, "{from:06X}: got {got:06X}, want {want:06X} (dE {e:.2})");
        }
        assert_eq!(DARK.map(0x000000), 0xF6F6F6);
        assert_eq!(DARK.map(0xFFFFFF), 0x262626);
    }

    #[test]
    fn highlights_follow_the_table_and_light_pages_keep_their_colours() {
        // The highlight colours as drawn (FONT-15) on a dark page, dark blue across the pure blues' step.
        for (from, want) in [
            (0xFFFF00, 0x515100),
            (0x00FF00, 0x006F00),
            (0x0000FF, 0x00A7FF),
            (0x00008B, 0xCCCCFF),
            (0x006400, 0x93CA93),
            (0xA9A9A9, 0x7A7A7A),
            (0xD3D3D3, 0x545454),
        ] {
            let got = DARK.map(from);
            assert!(de2000(got, want) <= 3.0, "{from:06X}: got {got:06X}, want {want:06X}");
        }
        assert_eq!(DARK.map(0x00008E), 0x49D1FF);
        for a in [Appearance::Light, Appearance::Dark { page: PageMode::Light }] {
            assert_eq!(a.map(0x123456), 0x123456);
        }
    }
}
