//! Theme colours and fonts

use crate::api::drawing::{ColourTransform, ColourValue, ThemeColour, TransformKind};
use crate::api::value::Color;
use crate::docx::ThemeColourRef;
use crate::docx::props::ThemeColours;

use super::Editor;

/// A WordprocessingML colour: the RGB it shows and the theme colour it follows, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordColour {
    /// What it shows.
    pub rgb: u32,
    /// The theme colour, with its tint or shade, it is written as.
    pub theme: Option<ThemeColourRef>,
}

impl WordColour {
    /// A plain RGB colour.
    pub fn rgb(rgb: u32) -> WordColour {
        WordColour { rgb: rgb & 0xFF_FFFF, theme: None }
    }
}

/// The colour value a WordprocessingML colour reads as: its theme colour with its tint or shade, else its RGB.
pub fn colour_value(rgb: u32, theme: Option<ThemeColourRef>) -> ColourValue {
    let Some((slot, t)) = theme.and_then(|t| ThemeColour::of(t.slot).map(|s| (s, t))) else { return ColourValue::rgb(Color(rgb)) };
    let mut v = ColourValue::theme(slot);
    let f = |b: u8| f64::from(b) / 255.0;
    match (t.tint, t.shade) {
        (Some(b), _) => v.transforms.push(ColourTransform { kind: TransformKind::Tint, value: f(b) }),
        (None, Some(b)) => v.transforms.push(ColourTransform { kind: TransformKind::Shade, value: f(b) }),
        _ => {}
    }
    v
}

/// The WordprocessingML colour `c` is over the theme `colours`: a theme colour takes at most one tint or shade, as Word
/// writes them; an RGB or preset colour is its RGB with its transforms applied.
pub fn word_colour(c: &ColourValue, colours: &ThemeColours, doc: &crate::docx::Document) -> Result<WordColour, String> {
    let Some(t) = c.theme else {
        let model = c.to_model()?;
        return Ok(WordColour::rgb(model.resolve(&doc.theme, &doc.colour_map, None).to_rgb()));
    };
    if c.rgb.is_some() || c.preset.is_some() {
        return Err("a colour has exactly one of theme, rgb and preset".into());
    }
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (tint, shade) = match c.transforms.as_slice() {
        [] => (None, None),
        [x] if x.kind == TransformKind::Tint => (Some(byte(x.value)), None),
        [x] if x.kind == TransformKind::Shade => (None, Some(byte(x.value))),
        _ => return Err("a theme colour here takes one tint or shade".into()),
    };
    let r = ThemeColourRef { slot: t.slot(), tint, shade };
    Ok(WordColour { rgb: colours.resolve(r), theme: Some(r) })
}

/// The theme colours Word's colour picker shows, in its column order.
pub const PICKER_COLUMNS: [ThemeColour; 10] = [
    ThemeColour::Background1,
    ThemeColour::Text1,
    ThemeColour::Background2,
    ThemeColour::Text2,
    ThemeColour::Accent1,
    ThemeColour::Accent2,
    ThemeColour::Accent3,
    ThemeColour::Accent4,
    ThemeColour::Accent5,
    ThemeColour::Accent6,
];

/// A theme colour's name as Word's colour picker gives it.
pub fn theme_colour_name(t: ThemeColour) -> &'static str {
    match t {
        ThemeColour::Text1 => "Text 1",
        ThemeColour::Background1 => "Background 1",
        ThemeColour::Text2 => "Text 2",
        ThemeColour::Background2 => "Background 2",
        ThemeColour::Accent1 => "Accent 1",
        ThemeColour::Accent2 => "Accent 2",
        ThemeColour::Accent3 => "Accent 3",
        ThemeColour::Accent4 => "Accent 4",
        ThemeColour::Accent5 => "Accent 5",
        ThemeColour::Accent6 => "Accent 6",
        ThemeColour::Hyperlink => "Hyperlink",
        ThemeColour::FollowedHyperlink => "Followed Hyperlink",
    }
}

/// One cell of the theme colour grid.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeSwatch {
    /// The colour as commands take it: the theme colour with its tint or shade.
    pub colour: ColourValue,
    /// What it shows.
    pub rgb: Color,
    /// Its name: "Accent 1", "Accent 1, Lighter 40%".
    pub label: String,
}

/// A column of the theme colour grid: the theme colour, then its five lighter and darker variants.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeColumn {
    /// The theme colour.
    pub theme: ThemeColour,
    /// The theme colour itself, then its variants, top to bottom.
    pub swatches: Vec<ThemeSwatch>,
}

/// The variants Word's grid offers under a colour of HSL luminance `l` (0 to 1): lighter by a percentage (a tint) or
/// darker (a shade).
fn variants(l: f64) -> [(bool, u8); 5] {
    if l <= 0.0 {
        [(true, 50), (true, 35), (true, 25), (true, 15), (true, 5)]
    } else if l >= 1.0 {
        [(false, 5), (false, 15), (false, 25), (false, 35), (false, 50)]
    } else if l < 0.2 {
        [(true, 90), (true, 75), (true, 50), (true, 25), (true, 10)]
    } else if l > 0.8 {
        [(false, 10), (false, 25), (false, 50), (false, 75), (false, 90)]
    } else {
        [(true, 80), (true, 60), (true, 40), (false, 25), (false, 50)]
    }
}

/// HSL luminance of `rgb`, 0 to 1.
fn luminance(rgb: u32) -> f64 {
    let [r, g, b] = [16, 8, 0].map(|s| f64::from((rgb >> s) & 0xFF) / 255.0);
    (r.max(g).max(b) + r.min(g).min(b)) / 2.0
}

/// The theme colour grid over `colours`: a column per picker colour.
pub fn theme_grid(colours: &ThemeColours) -> Vec<ThemeColumn> {
    PICKER_COLUMNS
        .iter()
        .map(|&theme| {
            let base = ThemeColourRef { slot: theme.slot(), tint: None, shade: None };
            let rgb = colours.resolve(base);
            let name = theme_colour_name(theme);
            let mut swatches = vec![ThemeSwatch { colour: ColourValue::theme(theme), rgb: Color(rgb), label: name.to_string() }];
            for (lighter, pct) in variants(luminance(rgb)) {
                let byte = ((1.0 - f64::from(pct) / 100.0) * 255.0).round() as u8;
                let r = if lighter { ThemeColourRef { tint: Some(byte), ..base } } else { ThemeColourRef { shade: Some(byte), ..base } };
                swatches.push(ThemeSwatch {
                    colour: colour_value(0, Some(r)),
                    rgb: Color(colours.resolve(r)),
                    label: format!("{name}, {} {pct}%", if lighter { "Lighter" } else { "Darker" }),
                });
            }
            ThemeColumn { theme, swatches }
        })
        .collect()
}

/// The theme's font families: its headings' and its body text's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeFontNames {
    /// The heading font (`+Headings`).
    pub heading: String,
    /// The body font (`+Body`).
    pub body: String,
}

impl Editor {
    /// The theme colours as they resolve through the document's colour mapping, for text and table colours.
    pub fn theme_palette(&self) -> ThemeColours {
        ThemeColours::new(&self.doc.theme, &self.doc.colour_map)
    }

    /// The WordprocessingML colour `c` is in this document ([`word_colour`]).
    pub fn word_colour(&self, c: &ColourValue) -> Result<WordColour, String> {
        word_colour(c, &self.theme_palette(), &self.doc)
    }

    /// What colour `c` shows in this document: a theme colour's tint or shade as Word applies one, other transforms
    /// as shapes apply them.
    #[cfg(test)]
    pub(crate) fn shown_colour(&self, c: &ColourValue) -> Result<Color, String> {
        if let Ok(w) = self.word_colour(c) {
            return Ok(Color(w.rgb));
        }
        Ok(Color(c.to_model()?.resolve(&self.doc.theme, &self.doc.colour_map, None).to_rgb()))
    }

    /// The theme colour grid Word's colour pickers show ([`theme_grid`]).
    pub fn theme_colour_grid(&self) -> Vec<ThemeColumn> {
        theme_grid(&self.theme_palette())
    }

    /// The theme's heading and body font families.
    pub fn theme_fonts(&self) -> ThemeFontNames {
        match &self.doc.resolver {
            Some(r) => ThemeFontNames { heading: r.theme.major.clone(), body: r.theme.minor.clone() },
            None => ThemeFontNames { heading: String::new(), body: String::new() },
        }
    }
}
