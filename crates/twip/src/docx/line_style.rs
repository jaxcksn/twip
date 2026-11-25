//! Line style, mark and shading values: `ST_Underline`, `ST_Border`, `ST_Em` and `ST_Shd`

use crate::document::format::{Emphasis, ShadingPattern, Stripe};
use crate::document::line::{ArtBorder, BorderStyle, Underline};

/// The underline a `w:u/@w:val` names: single for a value the format doesn't define.
pub fn underline(v: &str) -> Underline {
    use Underline::*;
    match v {
        "none" => None,
        "words" => Words,
        "double" => Double,
        "thick" => Thick,
        "dotted" => Dotted,
        "dottedHeavy" => DottedHeavy,
        "dash" => Dash,
        "dashedHeavy" => DashedHeavy,
        "dashLong" => DashLong,
        "dashLongHeavy" => DashLongHeavy,
        "dotDash" => DotDash,
        "dashDotHeavy" => DashDotHeavy,
        "dotDotDash" => DotDotDash,
        "dashDotDotHeavy" => DashDotDotHeavy,
        "wave" => Wave,
        "wavyHeavy" => WavyHeavy,
        "wavyDouble" => WavyDouble,
        _ => Single,
    }
}

/// The `w:u/@w:val` of `u`.
pub fn underline_val(u: Underline) -> &'static str {
    use Underline::*;
    match u {
        None => "none",
        Single => "single",
        Words => "words",
        Double => "double",
        Thick => "thick",
        Dotted => "dotted",
        DottedHeavy => "dottedHeavy",
        Dash => "dash",
        DashedHeavy => "dashedHeavy",
        DashLong => "dashLong",
        DashLongHeavy => "dashLongHeavy",
        DotDash => "dotDash",
        DashDotHeavy => "dashDotHeavy",
        DotDotDash => "dotDotDash",
        DashDotDotHeavy => "dashDotDotHeavy",
        Wave => "wave",
        WavyHeavy => "wavyHeavy",
        WavyDouble => "wavyDouble",
    }
}

/// The art borders' `ST_Border` values, in the format's order: an [`ArtBorder`]'s number is its place here.
const ART: [&str; 166] = [
    "apples",
    "archedScallops",
    "babyPacifier",
    "babyRattle",
    "balloons3Colors",
    "balloonsHotAir",
    "basicBlackDashes",
    "basicBlackDots",
    "basicBlackSquares",
    "basicThinLines",
    "basicWhiteDashes",
    "basicWhiteDots",
    "basicWhiteSquares",
    "basicWideInline",
    "basicWideMidline",
    "basicWideOutline",
    "bats",
    "birds",
    "birdsFlight",
    "cabins",
    "cakeSlice",
    "candyCorn",
    "celticKnotwork",
    "certificateBanner",
    "chainLink",
    "champagneBottle",
    "checkedBarBlack",
    "checkedBarColor",
    "checkered",
    "christmasTree",
    "circlesLines",
    "circlesRectangles",
    "classicalWave",
    "clocks",
    "compass",
    "confetti",
    "confettiGrays",
    "confettiOutline",
    "confettiStreamers",
    "confettiWhite",
    "cornerTriangles",
    "couponCutoutDashes",
    "couponCutoutDots",
    "crazyMaze",
    "creaturesButterfly",
    "creaturesFish",
    "creaturesInsects",
    "creaturesLadyBug",
    "crossStitch",
    "cup",
    "decoArch",
    "decoArchColor",
    "decoBlocks",
    "diamondsGray",
    "doubleD",
    "doubleDiamonds",
    "earth1",
    "earth2",
    "earth3",
    "eclipsingSquares1",
    "eclipsingSquares2",
    "eggsBlack",
    "fans",
    "film",
    "firecrackers",
    "flowersBlockPrint",
    "flowersDaisies",
    "flowersModern1",
    "flowersModern2",
    "flowersPansy",
    "flowersRedRose",
    "flowersRoses",
    "flowersTeacup",
    "flowersTiny",
    "gems",
    "gingerbreadMan",
    "gradient",
    "handmade1",
    "handmade2",
    "heartBalloon",
    "heartGray",
    "hearts",
    "heebieJeebies",
    "holly",
    "houseFunky",
    "hypnotic",
    "iceCreamCones",
    "lightBulb",
    "lightning1",
    "lightning2",
    "mapPins",
    "mapleLeaf",
    "mapleMuffins",
    "marquee",
    "marqueeToothed",
    "moons",
    "mosaic",
    "musicNotes",
    "northwest",
    "ovals",
    "packages",
    "palmsBlack",
    "palmsColor",
    "paperClips",
    "papyrus",
    "partyFavor",
    "partyGlass",
    "pencils",
    "people",
    "peopleWaving",
    "peopleHats",
    "poinsettias",
    "postageStamp",
    "pumpkin1",
    "pushPinNote2",
    "pushPinNote1",
    "pyramids",
    "pyramidsAbove",
    "quadrants",
    "rings",
    "safari",
    "sawtooth",
    "sawtoothGray",
    "scaredCat",
    "seattle",
    "shadowedSquares",
    "sharksTeeth",
    "shorebirdTracks",
    "skyrocket",
    "snowflakeFancy",
    "snowflakes",
    "sombrero",
    "southwest",
    "stars",
    "starsTop",
    "stars3d",
    "starsBlack",
    "starsShadowed",
    "sun",
    "swirligig",
    "tornPaper",
    "tornPaperBlack",
    "trees",
    "triangleParty",
    "triangles",
    "triangle1",
    "triangle2",
    "triangleCircle1",
    "triangleCircle2",
    "shapes1",
    "shapes2",
    "twistedLines1",
    "twistedLines2",
    "vine",
    "waveline",
    "weavingAngles",
    "weavingBraid",
    "weavingRibbon",
    "weavingStrips",
    "whiteFlowers",
    "woodwork",
    "xIllusions",
    "zanyTriangles",
    "zigZag",
    "zigZagStitch",
    "custom",
];

/// The art border a value the format doesn't define reads as: `custom`.
const UNKNOWN_ART: ArtBorder = ArtBorder(ART.len() as u8 - 1);

/// The border line a `w:val` (of `w:top`, `w:pBdr`'s sides, `w:bdr`, ...) names: an art border, as layout treats one,
/// for a value the format doesn't define.
pub fn border_style(v: &str) -> BorderStyle {
    use BorderStyle::*;
    match v {
        "nil" => Nil,
        "none" => None,
        "single" => Single,
        "thick" => Thick,
        "double" => Double,
        "dotted" => Dotted,
        "dashed" => Dashed,
        "dotDash" => DotDash,
        "dotDotDash" => DotDotDash,
        "triple" => Triple,
        "thinThickSmallGap" => ThinThickSmallGap,
        "thickThinSmallGap" => ThickThinSmallGap,
        "thinThickThinSmallGap" => ThinThickThinSmallGap,
        "thinThickMediumGap" => ThinThickMediumGap,
        "thickThinMediumGap" => ThickThinMediumGap,
        "thinThickThinMediumGap" => ThinThickThinMediumGap,
        "thinThickLargeGap" => ThinThickLargeGap,
        "thickThinLargeGap" => ThickThinLargeGap,
        "thinThickThinLargeGap" => ThinThickThinLargeGap,
        "wave" => Wave,
        "doubleWave" => DoubleWave,
        "dashSmallGap" => DashSmallGap,
        "dashDotStroked" => DashDotStroked,
        "threeDEmboss" => ThreeDEmboss,
        "threeDEngrave" => ThreeDEngrave,
        "outset" => Outset,
        "inset" => Inset,
        _ => Art(ART.iter().position(|a| *a == v).map_or(UNKNOWN_ART, |n| ArtBorder(n as u8))),
    }
}

/// The emphasis mark a `w:em/@w:val` names: a dot, as layout places one, for a value the format doesn't define.
pub fn emphasis(v: &str) -> Emphasis {
    match v {
        "none" => Emphasis::None,
        "comma" => Emphasis::Comma,
        "circle" => Emphasis::Circle,
        "underDot" => Emphasis::UnderDot,
        _ => Emphasis::Dot,
    }
}

/// The `w:em/@w:val` of `e`.
pub fn emphasis_val(e: Emphasis) -> &'static str {
    match e {
        Emphasis::None => "none",
        Emphasis::Dot => "dot",
        Emphasis::Comma => "comma",
        Emphasis::Circle => "circle",
        Emphasis::UnderDot => "underDot",
    }
}

/// The stripes and crosses of `ST_Shd`.
pub const STRIPES: [(Stripe, &str); 12] = [
    (Stripe::Horz, "horzStripe"),
    (Stripe::Vert, "vertStripe"),
    (Stripe::ReverseDiag, "reverseDiagStripe"),
    (Stripe::Diag, "diagStripe"),
    (Stripe::HorzCross, "horzCross"),
    (Stripe::DiagCross, "diagCross"),
    (Stripe::ThinHorz, "thinHorzStripe"),
    (Stripe::ThinVert, "thinVertStripe"),
    (Stripe::ThinReverseDiag, "thinReverseDiagStripe"),
    (Stripe::ThinDiag, "thinDiagStripe"),
    (Stripe::ThinHorzCross, "thinHorzCross"),
    (Stripe::ThinDiagCross, "thinDiagCross"),
];

/// The `w:shd/@w:val` of stripe `s`.
pub fn stripe_val(s: Stripe) -> &'static str {
    STRIPES.iter().find(|(t, _)| *t == s).map_or("clear", |(_, n)| n)
}

/// The pattern a `w:shd/@w:val` names, for a value the format doesn't define the one layout shows it as: a `pct` of
/// no percentage it can read shows nothing, anything else its fill.
pub fn shading_pattern(v: &str) -> ShadingPattern {
    match v {
        "nil" => ShadingPattern::Nil,
        "solid" => ShadingPattern::Solid,
        _ if v.starts_with("pct") => v[3..].parse::<u8>().ok().filter(|n| *n <= 100).map_or(ShadingPattern::Nil, ShadingPattern::Percent),
        _ => STRIPES.iter().find(|(_, n)| *n == v).map_or(ShadingPattern::Clear, |(s, _)| ShadingPattern::Stripe(*s)),
    }
}

/// The `w:shd/@w:val` of `p`.
pub fn shading_val(p: ShadingPattern) -> std::borrow::Cow<'static, str> {
    match p {
        ShadingPattern::Nil => "nil".into(),
        ShadingPattern::Clear => "clear".into(),
        ShadingPattern::Solid => "solid".into(),
        ShadingPattern::Percent(n) => format!("pct{n}").into(),
        ShadingPattern::Stripe(s) => stripe_val(s).into(),
    }
}

/// The `w:val` of border line `s`.
pub fn border_val(s: BorderStyle) -> &'static str {
    use BorderStyle::*;
    match s {
        Nil => "nil",
        None => "none",
        Single => "single",
        Thick => "thick",
        Double => "double",
        Dotted => "dotted",
        Dashed => "dashed",
        DotDash => "dotDash",
        DotDotDash => "dotDotDash",
        Triple => "triple",
        ThinThickSmallGap => "thinThickSmallGap",
        ThickThinSmallGap => "thickThinSmallGap",
        ThinThickThinSmallGap => "thinThickThinSmallGap",
        ThinThickMediumGap => "thinThickMediumGap",
        ThickThinMediumGap => "thickThinMediumGap",
        ThinThickThinMediumGap => "thinThickThinMediumGap",
        ThinThickLargeGap => "thinThickLargeGap",
        ThickThinLargeGap => "thickThinLargeGap",
        ThinThickThinLargeGap => "thinThickThinLargeGap",
        Wave => "wave",
        DoubleWave => "doubleWave",
        DashSmallGap => "dashSmallGap",
        DashDotStroked => "dashDotStroked",
        ThreeDEmboss => "threeDEmboss",
        ThreeDEngrave => "threeDEngrave",
        Outset => "outset",
        Inset => "inset",
        Art(a) => ART.get(a.0 as usize).copied().unwrap_or(ART[UNKNOWN_ART.0 as usize]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `ST_Underline` value.
    const UNDERLINES: [&str; 18] = [
        "single",
        "words",
        "double",
        "thick",
        "dotted",
        "dottedHeavy",
        "dash",
        "dashedHeavy",
        "dashLong",
        "dashLongHeavy",
        "dotDash",
        "dashDotHeavy",
        "dotDotDash",
        "dashDotDotHeavy",
        "wave",
        "wavyHeavy",
        "wavyDouble",
        "none",
    ];

    #[test]
    fn every_shading_pattern_round_trips_and_an_unknown_one_shows_as_layout_shows_it() {
        let pcts = [5, 10, 12, 15, 20, 25, 30, 35, 37, 40, 45, 50, 55, 60, 62, 65, 70, 75, 80, 85, 87, 90, 95].map(|n| format!("pct{n}"));
        let named = ["nil", "clear", "solid"].into_iter().chain(STRIPES.map(|(_, n)| n));
        for v in named.map(str::to_string).chain(pcts) {
            assert_eq!(shading_val(shading_pattern(&v)), v);
        }
        assert_eq!(
            (shading_pattern("bogus"), shading_pattern("pct500"), shading_pattern("pctx")),
            (ShadingPattern::Clear, ShadingPattern::Nil, ShadingPattern::Nil)
        );
    }

    #[test]
    fn every_emphasis_mark_round_trips_and_an_unknown_one_is_a_dot() {
        for v in ["none", "dot", "comma", "circle", "underDot"] {
            assert_eq!(emphasis_val(emphasis(v)), v);
        }
        assert_eq!(emphasis("sparkle"), Emphasis::Dot);
    }

    #[test]
    fn every_underline_round_trips() {
        let read: std::collections::HashSet<Underline> = UNDERLINES.iter().map(|v| underline(v)).collect();
        assert_eq!(read.len(), UNDERLINES.len(), "each value is its own underline");
        for v in UNDERLINES {
            assert_eq!(underline_val(underline(v)), v);
        }
        assert_eq!(underline("bogus"), Underline::Single);
    }

    /// Every `ST_Border` value that isn't an art border.
    const LINES: [&str; 27] = [
        "nil",
        "none",
        "single",
        "thick",
        "double",
        "dotted",
        "dashed",
        "dotDash",
        "dotDotDash",
        "triple",
        "thinThickSmallGap",
        "thickThinSmallGap",
        "thinThickThinSmallGap",
        "thinThickMediumGap",
        "thickThinMediumGap",
        "thinThickThinMediumGap",
        "thinThickLargeGap",
        "thickThinLargeGap",
        "thinThickThinLargeGap",
        "wave",
        "doubleWave",
        "dashSmallGap",
        "dashDotStroked",
        "threeDEmboss",
        "threeDEngrave",
        "outset",
        "inset",
    ];

    #[test]
    fn every_border_style_round_trips() {
        let all: Vec<&str> = LINES.iter().chain(&ART).copied().collect();
        let read: std::collections::HashSet<BorderStyle> = all.iter().map(|v| border_style(v)).collect();
        assert_eq!(read.len(), 193, "each value is its own style");
        for v in all {
            assert_eq!(border_val(border_style(v)), v);
        }
        assert_eq!(border_val(border_style("bogus")), "custom");
        assert!(matches!(border_style(""), BorderStyle::Art(_)));
    }
}
