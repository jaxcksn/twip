//! Printed Watermark and Fill Effects

use crate::api::command::{Command, PictureWatermark, TextWatermark, Watermark, WatermarkLayout};
use crate::api::drawing::{ColourTransform, ColourValue, FillValue, GradientShape, GradientStop, TransformKind};
use crate::api::error::Error;
use crate::api::services::{
    FieldInfo, FieldKind, FillEffectsDialog, FillEffectsTab, GradientColors, PrintedWatermarkDialog, ShadingStyle, WatermarkKind,
};
use crate::api::state::State;
use crate::api::value::{Bytes, Color};

use super::dialogs::{choice, field, number, off};

/// The texts Printed Watermark offers.
const TEXTS: [&str; 10] = ["ASAP", "CONFIDENTIAL", "COPY", "DO NOT COPY", "DRAFT", "ORIGINAL", "PERSONAL", "SAMPLE", "TOP SECRET", "URGENT"];
/// The sizes it offers, besides Auto.
const SIZES: [u32; 14] = [36, 40, 44, 48, 54, 60, 66, 72, 80, 90, 96, 105, 120, 144];
/// The scales it offers, besides Auto.
const SCALES: [u32; 5] = [500, 200, 150, 100, 50];

/// Word's preset gradients: name, label and stops (position in 100000ths, colour), as Word writes each.
const PRESETS: [(&str, &str, &[(u32, u32)]); 24] = [
    ("earlySunset", "Early Sunset", &[(0, 0x000082), (30000, 0x66008F), (64999, 0xBA0066), (89999, 0xFF0000), (100000, 0xFF8200)]),
    ("lateSunset", "Late Sunset", &[(0, 0x000000), (20000, 0x000040), (50000, 0x400040), (75000, 0x8F0040), (89999, 0xF27300), (100000, 0xFFBF00)]),
    ("nightfall", "Nightfall", &[(0, 0x000000), (39999, 0x0A128C), (70000, 0x181CC7), (88000, 0x7005D4), (100000, 0x8C3D91)]),
    ("daybreak", "Daybreak", &[(0, 0x5E9EFF), (39999, 0x85C2FF), (70000, 0xC4D6EB), (100000, 0xFFEBFA)]),
    (
        "horizon",
        "Horizon",
        &[
            (0, 0xDCEBF5),
            (8000, 0x83A7C3),
            (13000, 0x768FB9),
            (21001, 0x83A7C3),
            (52000, 0xFFFFFF),
            (56000, 0x9C6563),
            (58000, 0x80302D),
            (71001, 0xC0524E),
            (94000, 0xEBDAD4),
            (100000, 0x55261C),
        ],
    ),
    (
        "desert",
        "Desert",
        &[
            (0, 0xFC9FCB),
            (13000, 0xF8B049),
            (21001, 0xF8B049),
            (63000, 0xFEE7F2),
            (67000, 0xF952A0),
            (69000, 0xC50849),
            (82001, 0xB43E85),
            (100000, 0xF8B049),
        ],
    ),
    ("ocean", "Ocean", &[(0, 0x03D4A8), (25000, 0x21D6E0), (75000, 0x0087E6), (100000, 0x005CBF)]),
    ("calmWater", "Calm Water", &[(0, 0xCCCCFF), (17999, 0x99CCFF), (36000, 0x9966FF), (61000, 0xCC99FF), (82001, 0x99CCFF), (100000, 0xCCCCFF)]),
    ("fire", "Fire", &[(0, 0xFFF200), (45000, 0xFF7A00), (70000, 0xFF0300), (100000, 0x4D0808)]),
    ("fog", "Fog", &[(0, 0x8488C4), (53000, 0xD4DEFF), (83000, 0xD4DEFF), (100000, 0x96AB94)]),
    ("moss", "Moss", &[(0, 0xDDEBCF), (50000, 0x9CB86E), (100000, 0x156B13)]),
    (
        "peacock",
        "Peacock",
        &[(0, 0x3399FF), (16000, 0x00CCCC), (47000, 0x9999FF), (60001, 0x2E6792), (71001, 0x3333CC), (81000, 0x1170FF), (100000, 0x006699)],
    ),
    ("wheat", "Wheat", &[(0, 0xFBEAC7), (17999, 0xFEE7F2), (36000, 0xFAC77D), (61000, 0xFBA97D), (82001, 0xFBD49C), (100000, 0xFEE7F2)]),
    ("parchment", "Parchment", &[(0, 0xFFEFD1), (64999, 0xF0EBD5), (100000, 0xD1C39F)]),
    ("mahogany", "Mahogany", &[(0, 0xD6B19C), (30000, 0xD49E6C), (70000, 0xA65528), (100000, 0x663012)]),
    (
        "rainbow",
        "Rainbow",
        &[(0, 0xA603AB), (21001, 0x0819FB), (35001, 0x1A8D48), (52000, 0xFFFF00), (73000, 0xEE3F17), (88000, 0xE81766), (100000, 0xA603AB)],
    ),
    ("rainbow2", "Rainbow II", &[(0, 0xFF3399), (25000, 0xFF6633), (50000, 0xFFFF00), (75000, 0x01A78F), (100000, 0x3366FF)]),
    ("gold", "Gold", &[(0, 0xE6DCAC), (12000, 0xE6D78A), (30000, 0xC7AC4C), (45000, 0xE6D78A), (77000, 0xC7AC4C), (100000, 0xE6DCAC)]),
    (
        "gold2",
        "Gold II",
        &[
            (0, 0xFBE4AE),
            (13000, 0xBD922A),
            (21001, 0xBD922A),
            (63000, 0xFBE4AE),
            (67000, 0xBD922A),
            (69000, 0x835E17),
            (82001, 0xA28949),
            (100000, 0xFAE3B7),
        ],
    ),
    (
        "brass",
        "Brass",
        &[
            (0, 0x825600),
            (13000, 0xFFA800),
            (28000, 0x825600),
            (42999, 0xFFA800),
            (58000, 0x825600),
            (72000, 0xFFA800),
            (87000, 0x825600),
            (100000, 0xFFA800),
        ],
    ),
    (
        "chrome",
        "Chrome",
        &[
            (0, 0xFFFFFF),
            (16000, 0x1F1F1F),
            (17999, 0xFFFFFF),
            (42000, 0x636363),
            (53000, 0xCFCFCF),
            (66000, 0xCFCFCF),
            (75999, 0x1F1F1F),
            (78999, 0xFFFFFF),
            (100000, 0x7F7F7F),
        ],
    ),
    (
        "chrome2",
        "Chrome II",
        &[
            (0, 0xCBCBCB),
            (13000, 0x5F5F5F),
            (21001, 0x5F5F5F),
            (63000, 0xFFFFFF),
            (67000, 0xB2B2B2),
            (69000, 0x292929),
            (82001, 0x777777),
            (100000, 0xEAEAEA),
        ],
    ),
    ("silver", "Silver", &[(0, 0xFFFFFF), (7001, 0xE6E6E6), (32001, 0x7D8496), (47000, 0xE6E6E6), (85001, 0x7D8496), (100000, 0xE6E6E6)]),
    (
        "sapphire",
        "Sapphire",
        &[
            (0, 0x000082),
            (13000, 0x0047FF),
            (28000, 0x000082),
            (42999, 0x0047FF),
            (58000, 0x000082),
            (72000, 0x0047FF),
            (87000, 0x000082),
            (100000, 0x0047FF),
        ],
    ),
];

/// Printed Watermark's values: the watermark the selection's pages show, else the dialog's defaults.
pub fn watermark_values(state: &State) -> PrintedWatermarkDialog {
    let defaults = PrintedWatermarkDialog {
        watermark: WatermarkKind::None,
        picture: Bytes(Vec::new()),
        scale: None,
        washout: true,
        text: TEXTS[0].into(),
        font: String::new(),
        language: state.spelling.language.clone(),
        size: None,
        color: Color(crate::docx::watermark::SILVER),
        transparency: 50.0,
        layout: WatermarkLayout::Diagonal,
        bold: false,
        italic: false,
    };
    match &state.page.watermark {
        None => defaults,
        Some(Watermark::Picture(p)) => {
            PrintedWatermarkDialog { watermark: WatermarkKind::Picture, scale: p.scale, washout: p.washout.unwrap_or(true), ..defaults }
        }
        Some(Watermark::Text(t)) => PrintedWatermarkDialog {
            watermark: WatermarkKind::Text,
            text: t.text.clone(),
            font: t.font.clone().unwrap_or(defaults.font),
            language: t.language.clone().unwrap_or(defaults.language),
            size: t.size,
            color: t.color.unwrap_or(defaults.color),
            transparency: t.transparency.unwrap_or(50.0),
            layout: t.layout.unwrap_or_default(),
            bold: t.bold.unwrap_or(false),
            italic: t.italic.unwrap_or(false),
            ..defaults
        },
    }
}

/// Printed Watermark's fields: the picture's for a picture watermark, the text's for a text one.
pub fn watermark_fields(v: &PrintedWatermarkDialog) -> Vec<(&'static str, FieldInfo)> {
    let on = |f: FieldInfo, yes: bool| if yes { f } else { off(f) };
    let (picture, text) = (v.watermark == WatermarkKind::Picture, v.watermark == WatermarkKind::Text);
    let texts: Vec<(&str, &str)> = TEXTS.iter().map(|t| (*t, *t)).collect();
    let sizes: Vec<(String, String)> =
        std::iter::once(("".into(), "Auto".into())).chain(SIZES.iter().map(|s| (s.to_string(), s.to_string()))).collect();
    let scales: Vec<(String, String)> =
        std::iter::once(("".into(), "Auto".into())).chain(SCALES.iter().map(|s| (s.to_string(), format!("{s}%")))).collect();
    let with_options = |f: FieldInfo, o: &[(&str, &str)]| FieldInfo { options: choice("", o).options, ..f };
    vec![
        ("watermark", choice("Watermark", &[("none", "No watermark"), ("picture", "Picture watermark"), ("text", "Text watermark")])),
        ("picture", on(field("Select Picture", FieldKind::Picture), picture)),
        ("scale", on(with_options(number("Scale", FieldKind::Number, 1.0, 5000.0, 1.0), &pairs(&scales)), picture)),
        ("washout", on(field("Washout", FieldKind::Toggle), picture)),
        ("text", on(with_options(field("Text", FieldKind::Text), &texts), text)),
        ("font", on(field("Font", FieldKind::Text), text)),
        ("language", on(field("Language", FieldKind::Text), text)),
        ("size", on(with_options(number("Size", FieldKind::Length, 1.0, 1584.0, 1.0), &pairs(&sizes)), text)),
        ("color", on(field("Color", FieldKind::Color), text)),
        ("transparency", on(number("Transparency", FieldKind::Number, 0.0, 100.0, 1.0), text)),
        ("layout", on(choice("Layout", &[("diagonal", "Diagonal"), ("horizontal", "Horizontal")]), text)),
        ("bold", on(field("Bold", FieldKind::Toggle), text)),
        ("italic", on(field("Italic", FieldKind::Toggle), text)),
    ]
}

fn pairs(v: &[(String, String)]) -> Vec<(&str, &str)> {
    v.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect()
}

/// The command Printed Watermark runs when its values changed from `was`.
pub fn watermark_commands(v: &PrintedWatermarkDialog, was: &PrintedWatermarkDialog) -> Result<Vec<Command>, Error> {
    if v == was {
        return Ok(Vec::new());
    }
    Ok(vec![match v.watermark {
        WatermarkKind::None => Command::RemoveWatermark,
        WatermarkKind::Picture => {
            if v.picture.0.is_empty() && was.watermark != WatermarkKind::Picture {
                return Err(Error::invalid("select a picture"));
            }
            Command::SetWatermark(Watermark::Picture(PictureWatermark { image: v.picture.clone(), scale: v.scale, washout: Some(v.washout) }))
        }
        WatermarkKind::Text => {
            if v.text.trim().is_empty() {
                return Err(Error::invalid("the watermark needs text"));
            }
            Command::SetWatermark(Watermark::Text(TextWatermark {
                text: v.text.clone(),
                font: Some(v.font.trim().to_string()).filter(|f| !f.is_empty()),
                language: Some(v.language.trim().to_string()).filter(|l| !l.is_empty()),
                size: v.size,
                color: Some(v.color),
                transparency: Some(v.transparency),
                layout: Some(v.layout),
                bold: Some(v.bold),
                italic: Some(v.italic),
            }))
        }
    }])
}

fn rgb(c: u32) -> ColourValue {
    ColourValue::rgb(Color(c))
}

/// A gradient's base angle for a linear shading style, degrees (0 left to right, 90 top to bottom).
fn base_angle(s: ShadingStyle) -> Option<f64> {
    match s {
        ShadingStyle::Horizontal => Some(90.0),
        ShadingStyle::Vertical => Some(0.0),
        ShadingStyle::DiagonalUp => Some(45.0),
        ShadingStyle::DiagonalDown => Some(135.0),
        ShadingStyle::FromCorner | ShadingStyle::FromCenter => None,
    }
}

/// The corner a From corner variant starts in, as a focus rectangle's insets (left, top, right, bottom).
fn corner(variant: u32) -> [f64; 4] {
    match variant {
        2 => [1.0, 0.0, 0.0, 1.0],
        3 => [0.0, 1.0, 1.0, 0.0],
        4 => [1.0, 1.0, 0.0, 0.0],
        _ => [0.0, 0.0, 1.0, 1.0],
    }
}

fn reversed(stops: &[GradientStop]) -> Vec<GradientStop> {
    stops.iter().rev().map(|s| GradientStop { position: 1.0 - s.position, colour: s.colour.clone() }).collect()
}

/// The stops pressed into the first half and mirrored into the second.
fn mirrored(stops: &[GradientStop]) -> Vec<GradientStop> {
    let half: Vec<GradientStop> = stops.iter().map(|s| GradientStop { position: s.position / 2.0, colour: s.colour.clone() }).collect();
    let back = half.iter().rev().skip(1).map(|s| GradientStop { position: 1.0 - s.position, colour: s.colour.clone() });
    half.iter().cloned().chain(back).collect()
}

/// The gradient Fill Effects' values make, as Word makes each style and variant of it.
fn gradient(v: &FillEffectsDialog) -> Result<FillValue, Error> {
    let stops: Vec<GradientStop> = match v.colors {
        GradientColors::One => {
            let d = v.lightness.clamp(0.0, 1.0);
            let mut second = v.color1.clone();
            if d > 0.5 {
                second.transforms.push(ColourTransform { kind: TransformKind::Tint, value: 2.0 * (1.0 - d) });
            } else if d < 0.5 {
                second.transforms.push(ColourTransform { kind: TransformKind::Shade, value: 2.0 * d });
            }
            vec![GradientStop { position: 0.0, colour: v.color1.clone() }, GradientStop { position: 1.0, colour: second }]
        }
        GradientColors::Two => {
            vec![GradientStop { position: 0.0, colour: v.color1.clone() }, GradientStop { position: 1.0, colour: v.color2.clone() }]
        }
        GradientColors::Preset => {
            let (_, _, list) = PRESETS.iter().find(|(n, _, _)| *n == v.preset).ok_or_else(|| Error::invalid("not a preset gradient"))?;
            list.iter().map(|(p, c)| GradientStop { position: f64::from(*p) / 100000.0, colour: rgb(*c) }).collect()
        }
    };
    let variant = v.variant.clamp(1, if v.shading == ShadingStyle::FromCenter { 2 } else { 4 });
    Ok(match base_angle(v.shading) {
        Some(a) => {
            let (angle, stops) = match variant {
                1 => (a, stops),
                2 => (a + 180.0, stops),
                3 => (a, mirrored(&stops)),
                _ => (a + 180.0, mirrored(&reversed(&stops))),
            };
            FillValue::Gradient { kind: GradientShape::Linear, angle: Some(angle.rem_euclid(360.0)), stops, focus: None }
        }
        None if v.shading == ShadingStyle::FromCorner => {
            FillValue::Gradient { kind: GradientShape::Rectangular, angle: None, stops, focus: Some(corner(variant)) }
        }
        None => {
            let stops = if variant == 2 { reversed(&stops) } else { stops };
            FillValue::Gradient { kind: GradientShape::Rectangular, angle: None, stops, focus: None }
        }
    })
}

/// Fill Effects' values for the page's fill: its gradient (style and variant as Word makes them), texture, pattern or
/// picture; else a gradient of the page colour.
pub fn fill_effects_values(state: &State) -> FillEffectsDialog {
    let white = rgb(0xFF_FFFF);
    let mut v = FillEffectsDialog {
        tab: FillEffectsTab::Gradient,
        colors: GradientColors::One,
        color1: white.clone(),
        color2: white.clone(),
        lightness: 0.5,
        preset: PRESETS[0].0.into(),
        shading: ShadingStyle::Horizontal,
        variant: 1,
        texture: Bytes(Vec::new()),
        pattern: crate::drawing::patterns::WORD[0].0.name().into(),
        foreground: rgb(0),
        background: white,
        picture: Bytes(Vec::new()),
    };
    match &state.page.background {
        Some(FillValue::Solid { colour }) => v.color1 = colour.clone(),
        Some(FillValue::Pattern { preset, foreground, background }) => {
            (v.tab, v.pattern, v.foreground, v.background) = (FillEffectsTab::Pattern, preset.clone(), foreground.clone(), background.clone());
        }
        Some(FillValue::Picture { tile, .. }) => v.tab = if tile.unwrap_or(false) { FillEffectsTab::Texture } else { FillEffectsTab::Picture },
        Some(FillValue::Gradient { kind, angle, stops, focus }) if stops.len() >= 2 => {
            // Two colours as Word blends them (how they read) are the two.
            let stops = &two_stops(stops).unwrap_or_else(|| stops.clone());
            let g = &FillValue::Gradient { kind: *kind, angle: *angle, stops: stops.clone(), focus: *focus };
            (v.color1, v.color2) = (stops[0].colour.clone(), stops[stops.len() - 1].colour.clone());
            v.colors = GradientColors::Two;
            // The style and variant (and preset) that make this gradient, when one does.
            let styles = [
                ShadingStyle::Horizontal,
                ShadingStyle::Vertical,
                ShadingStyle::DiagonalUp,
                ShadingStyle::DiagonalDown,
                ShadingStyle::FromCorner,
                ShadingStyle::FromCenter,
            ];
            let presets = std::iter::once(None).chain(PRESETS.iter().map(|p| Some(p.0)));
            for preset in presets {
                for shading in styles {
                    for variant in 1..=4 {
                        let (a, b, m) = (&stops[0].colour, &stops[stops.len() - 1].colour, &stops[stops.len() / 2].colour);
                        for (color1, color2) in [(a, b), (b, a), (a, m), (m, a)].map(|(x, y)| (x.clone(), y.clone())) {
                            let mut try_v = FillEffectsDialog { shading, variant, color1, color2, ..v.clone() };
                            if let Some(p) = preset {
                                (try_v.colors, try_v.preset) = (GradientColors::Preset, p.into());
                            }
                            if gradient(&try_v).is_ok_and(|made| same_gradient(&made, g)) {
                                return one_colour(try_v);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    v
}

/// The two colours of stops that are Word's blend of them.
fn two_stops(stops: &[GradientStop]) -> Option<Vec<GradientStop>> {
    let rgb = |s: &GradientStop| s.colour.rgb.filter(|_| s.colour.theme.is_none() && s.colour.preset.is_none() && s.colour.transforms.is_empty());
    let (a, b) = (rgb(stops.first()?)?, rgb(stops.last()?)?);
    let blend = crate::drawing::vml::two_colour(&crate::drawing::colour::Colour::srgb(a.0), &crate::drawing::colour::Colour::srgb(b.0));
    let same = blend.len() == stops.len()
        && blend.iter().zip(stops).all(|((p, c), s)| (p.fraction() - s.position).abs() < 1e-4 && ColourValue::of(c, None) == s.colour);
    same.then(|| vec![stops[0].clone(), stops[stops.len() - 1].clone()])
}

/// Two colours, the second the first's VML `darken` or `lighten`, as One colour with its lightness.
fn one_colour(v: FillEffectsDialog) -> FillEffectsDialog {
    let plain = |c: &ColourValue| c.rgb.filter(|_| c.theme.is_none() && c.preset.is_none() && c.transforms.is_empty());
    let (Some(a), Some(b), GradientColors::Two) = (plain(&v.color1), plain(&v.color2), v.colors) else { return v };
    match crate::drawing::vml::shade_of(a.0, b.0) {
        Some((light, n)) => {
            let n = f64::from(n) / 510.0;
            FillEffectsDialog { colors: GradientColors::One, lightness: if light { 1.0 - n } else { n }, ..v }
        }
        None => v,
    }
}

/// Whether two gradients are the same to the stop (positions to 1/1000).
fn same_gradient(a: &FillValue, b: &FillValue) -> bool {
    let (FillValue::Gradient { kind: ka, angle: aa, stops: sa, focus: fa }, FillValue::Gradient { kind: kb, angle: ab, stops: sb, focus: fb }) =
        (a, b)
    else {
        return false;
    };
    let near = |x: f64, y: f64| (x - y).abs() < 1e-3;
    let angle = |a: &Option<f64>| a.unwrap_or(0.0).rem_euclid(360.0);
    let focus = |f: &Option<[f64; 4]>| f.unwrap_or([0.5; 4]);
    ka == kb
        && (*ka != GradientShape::Linear || near(angle(aa), angle(ab)))
        && focus(fa).iter().zip(focus(fb)).all(|(x, y)| near(*x, y))
        && sa.len() == sb.len()
        && sa.iter().zip(sb).all(|(x, y)| near(x.position, y.position) && x.colour == y.colour)
}

/// Fill Effects' fields: each tab's, enabled on its tab.
pub fn fill_effects_fields(v: &FillEffectsDialog) -> Vec<(&'static str, FieldInfo)> {
    let on = |f: FieldInfo, yes: bool| if yes { f } else { off(f) };
    let g = v.tab == FillEffectsTab::Gradient;
    let presets: Vec<(&str, &str)> = PRESETS.iter().map(|(n, l, _)| (*n, *l)).collect();
    let patterns: Vec<(&str, &str)> = crate::drawing::patterns::WORD.iter().map(|(p, l)| (p.name(), *l)).collect();
    let styles = [
        ("horizontal", "Horizontal"),
        ("vertical", "Vertical"),
        ("diagonalUp", "Diagonal up"),
        ("diagonalDown", "Diagonal down"),
        ("fromCorner", "From corner"),
        ("fromCenter", "From center"),
    ];
    let variants = if v.shading == ShadingStyle::FromCenter { 2.0 } else { 4.0 };
    vec![
        ("tab", choice("Fill Effects", &[("gradient", "Gradient"), ("texture", "Texture"), ("pattern", "Pattern"), ("picture", "Picture")])),
        ("colors", on(choice("Colors", &[("one", "One color"), ("two", "Two colors"), ("preset", "Preset")]), g)),
        ("color1", on(field("Color 1", FieldKind::Color), g && v.colors != GradientColors::Preset)),
        ("color2", on(field("Color 2", FieldKind::Color), g && v.colors == GradientColors::Two)),
        ("lightness", on(number("Dark to Light", FieldKind::Number, 0.0, 1.0, 0.01), g && v.colors == GradientColors::One)),
        ("preset", on(choice("Preset colors", &presets), g && v.colors == GradientColors::Preset)),
        ("shading", on(choice("Shading styles", &styles), g)),
        ("variant", on(number("Variants", FieldKind::Number, 1.0, variants, 1.0), g)),
        ("texture", on(field("Other Texture", FieldKind::Picture), v.tab == FillEffectsTab::Texture)),
        ("pattern", on(choice("Pattern", &patterns), v.tab == FillEffectsTab::Pattern)),
        ("foreground", on(field("Foreground", FieldKind::Color), v.tab == FillEffectsTab::Pattern)),
        ("background", on(field("Background", FieldKind::Color), v.tab == FillEffectsTab::Pattern)),
        ("picture", on(field("Select Picture", FieldKind::Picture), v.tab == FillEffectsTab::Picture)),
    ]
}

/// The command Fill Effects runs when its values changed from `was`: the page's fill from the tab shown.
pub fn fill_effects_commands(v: &FillEffectsDialog, was: &FillEffectsDialog) -> Result<Vec<Command>, Error> {
    if v == was {
        return Ok(Vec::new());
    }
    // No picture chosen keeps the page's, when it has one.
    let has_picture = matches!(was.tab, FillEffectsTab::Texture | FillEffectsTab::Picture);
    let picture = |b: &Bytes, tile: bool| match b.0.is_empty() && !has_picture {
        true => Err(Error::invalid("select a picture")),
        false => Ok(FillValue::Picture { image: b.clone(), tile: Some(tile) }),
    };
    let fill = match v.tab {
        FillEffectsTab::Gradient => gradient(v)?,
        // shortcut: Word's own textures are its pictures, which can't ship; a texture is a picture the user gives.
        FillEffectsTab::Texture => picture(&v.texture, true)?,
        FillEffectsTab::Picture => picture(&v.picture, false)?,
        FillEffectsTab::Pattern => {
            FillValue::Pattern { preset: v.pattern.clone(), foreground: v.foreground.clone(), background: v.background.clone() }
        }
    };
    Ok(vec![Command::SetPageColor(fill)])
}
