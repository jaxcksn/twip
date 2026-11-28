//! Theme and colour map

use super::colour::{Colour, ColourBase, ColourMod, Rgba, SchemeSlot};
use super::paint::{Cap, Compound, Dash, EffectList, Fill, Gradient, GradientKind, Join, Line, OuterShadow, PresetDash, RectAlign, TileFlip};
use super::units::{Angle, Emu, Pct};

/// A document's theme: its colours, and the fills, lines and effects shape styles take from it.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Its colours: dark 1, light 1, dark 2, light 2, accents 1 to 6, hyperlink and followed hyperlink.
    pub colours: [Rgba; 12],
    /// Its fills, subtle to intense: a style's fill references 1 to 1000.
    pub fills: Vec<Fill>,
    /// Its lines, subtle to intense.
    pub lines: Vec<Line>,
    /// Its effects, subtle to intense.
    pub effects: Vec<EffectList>,
    /// Its background fills: a style's fill references from 1001.
    pub bg_fills: Vec<Fill>,
}

const DEFAULT_COLOURS: [u32; 12] =
    [0x000000, 0xFFFFFF, 0x0E2841, 0xE8E8E8, 0x156082, 0xE97132, 0x196B24, 0x0F9ED5, 0xA02B93, 0x4EA72E, 0x467886, 0x96607D];

impl Default for Theme {
    /// The Office theme: its colours and its format scheme's fills, lines and effects.
    fn default() -> Theme {
        use ColourMod::{LumMod, SatMod, Shade, Tint};
        let ph = |mods: &[ColourMod]| Colour { base: ColourBase::Scheme(SchemeSlot::Placeholder), mods: mods.to_vec() };
        let gradient = |stops: [&[ColourMod]; 3]| {
            Fill::Gradient(Gradient {
                stops: [0, 50000, 100000].into_iter().zip(stops).map(|(p, m)| (Pct(p), ph(m))).collect(),
                kind: GradientKind::Linear { angle: Angle(5_400_000), scaled: false },
                tile: [Pct(0); 4],
                flip: TileFlip::None,
                rot_with_shape: true,
            })
        };
        let line = |w: i64| Line {
            width: Some(Emu(w)),
            fill: Some(Fill::Solid(ph(&[]))),
            dash: Some(Dash::Preset(PresetDash::Solid)),
            compound: Some(Compound::Single),
            cap: Some(Cap::Flat),
            join: Some(Join::Miter(Pct(800_000))),
            inset: Some(false),
            head: None,
            tail: None,
        };
        let shadow = OuterShadow {
            blur: Emu(57150),
            dist: Emu(19050),
            dir: Angle(5_400_000),
            sx: Pct::ONE,
            sy: Pct::ONE,
            kx: Angle(0),
            ky: Angle(0),
            align: RectAlign::Ctr,
            colour: Colour::srgb(0).with(ColourMod::Alpha(Pct(63000))),
            rot_with_shape: false,
        };
        Theme {
            colours: DEFAULT_COLOURS.map(Rgba::rgb),
            fills: vec![
                Fill::Solid(ph(&[])),
                gradient([
                    &[LumMod(Pct(110000)), SatMod(Pct(105000)), Tint(Pct(67000))],
                    &[LumMod(Pct(105000)), SatMod(Pct(103000)), Tint(Pct(73000))],
                    &[LumMod(Pct(105000)), SatMod(Pct(109000)), Tint(Pct(81000))],
                ]),
                gradient([
                    &[SatMod(Pct(103000)), LumMod(Pct(102000)), Tint(Pct(94000))],
                    &[SatMod(Pct(110000)), LumMod(Pct(100000)), Shade(Pct(100000))],
                    &[LumMod(Pct(99000)), SatMod(Pct(120000)), Shade(Pct(78000))],
                ]),
            ],
            lines: vec![line(6350), line(12700), line(19050)],
            effects: vec![EffectList::default(), EffectList::default(), EffectList { outer: Some(shadow), ..EffectList::default() }],
            bg_fills: vec![
                Fill::Solid(ph(&[])),
                Fill::Solid(ph(&[Tint(Pct(95000)), SatMod(Pct(170000))])),
                gradient([
                    &[Tint(Pct(93000)), SatMod(Pct(150000)), Shade(Pct(98000)), LumMod(Pct(102000))],
                    &[Tint(Pct(98000)), SatMod(Pct(130000)), Shade(Pct(90000)), LumMod(Pct(103000))],
                    &[Shade(Pct(63000)), SatMod(Pct(120000))],
                ]),
            ],
        }
    }
}

impl Theme {
    /// Its colour in `slot`; black for a slot the colour map names (background, text, the placeholder).
    pub fn colour(&self, slot: SchemeSlot) -> Rgba {
        slot.theme_index().map_or(Rgba::BLACK, |i| self.colours[i])
    }
}

/// Which theme colour each colour role takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColourMap {
    /// Background 1's.
    pub bg1: SchemeSlot,
    /// Text 1's.
    pub tx1: SchemeSlot,
    /// Background 2's.
    pub bg2: SchemeSlot,
    /// Text 2's.
    pub tx2: SchemeSlot,
    /// Accents 1 to 6's.
    pub accents: [SchemeSlot; 6],
    /// The hyperlink's.
    pub hlink: SchemeSlot,
    /// The followed hyperlink's.
    pub fol_hlink: SchemeSlot,
}

impl Default for ColourMap {
    fn default() -> ColourMap {
        use SchemeSlot::*;
        ColourMap {
            bg1: Lt1,
            tx1: Dk1,
            bg2: Lt2,
            tx2: Dk2,
            accents: [Accent(1), Accent(2), Accent(3), Accent(4), Accent(5), Accent(6)],
            hlink: Hlink,
            fol_hlink: FolHlink,
        }
    }
}

impl ColourMap {
    /// The theme colour role `s` takes.
    pub fn map(&self, s: SchemeSlot) -> SchemeSlot {
        use SchemeSlot::*;
        match s {
            Bg1 => self.bg1,
            Tx1 => self.tx1,
            Bg2 => self.bg2,
            Tx2 => self.tx2,
            Accent(n) => self.accents[(n as usize).clamp(1, 6) - 1],
            Hlink => self.hlink,
            FolHlink => self.fol_hlink,
            other => other,
        }
    }
}
