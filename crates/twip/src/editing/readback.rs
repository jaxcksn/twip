//! Formatting read-back

use crate::api::command::{BorderStyle, ShadingPattern};
use crate::api::drawing::ColourValue;
use crate::api::state::Tri;
use crate::api::value::{AutoOr, NoneOr, Points};
use crate::docx::props::{Border, BorderStyle as Line, Shd};
use crate::docx::{ParaStyle, RunStyle, ThemeFont};

use super::state::fold;
use super::theme::{WordColour, colour_value};
use crate::api::command::LigatureFlags;

/// The most a border line's distance from the text is: Word's 31 pt (`ST_PointMeasure` allows no more).
pub const MAX_BORDER_SPACE: f64 = 31.0;

/// A border line: its style, width, colour, distance from what it borders, and whether it has a shadow.
#[derive(Clone, Debug, PartialEq)]
pub struct BorderLine {
    /// The line style.
    pub style: BorderStyle,
    /// The width, 0.25 to 6 pt (a picture border's, 1 to 31 pt).
    pub width: Points,
    /// The colour: RGB or a theme colour with a tint or shade, or automatic.
    pub color: AutoOr<ColourValue>,
    /// The distance from the text (from the page's edge, for a page border measured from it), 0 to 31 pt.
    pub space: Points,
    /// A shadow.
    pub shadow: bool,
}

impl From<BorderLine> for crate::api::command::Border {
    fn from(b: BorderLine) -> crate::api::command::Border {
        crate::api::command::Border { style: b.style, width: b.width, color: b.color, space: Some(b.space), shadow: Some(b.shadow) }
    }
}

impl BorderLine {
    /// An API border line: its distance `space` and shadow `shadow` where it gives none.
    pub fn of_api(b: &crate::api::command::Border, space: f64, shadow: bool) -> BorderLine {
        BorderLine {
            style: b.style,
            width: b.width,
            color: b.color.clone(),
            space: b.space.unwrap_or(Points(space)),
            shadow: b.shadow.unwrap_or(shadow),
        }
    }

    /// Within Word's ranges: the width 0.25 to 6 pt, the distance 0 to 31 pt.
    pub fn check(&self) -> Result<(), String> {
        let ok = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !ok(self.width.0, 0.25, 6.0) {
            return Err("the width must be from 0.25 to 6".into());
        }
        if !ok(self.space.0, 0.0, MAX_BORDER_SPACE) {
            return Err(format!("the distance from the text must be from 0 to {MAX_BORDER_SPACE}"));
        }
        Ok(())
    }

    /// The `name` element (`w:bdr`, `w:top`) it is written as, its colour `c`.
    pub fn xml(&self, w: &str, name: &str, c: Option<WordColour>) -> String {
        element(w, name, border_style(self.style), self.width.0, self.space.0, self.shadow, c)
    }

    /// A model border as read back; none for one that draws nothing.
    pub fn of(b: &Border) -> Option<BorderLine> {
        b.visible().then(|| BorderLine {
            style: api_border_style(b.style),
            width: Points(b.width),
            color: b.color.map_or(AutoOr::Auto, |c| AutoOr::Value(colour_value(c, b.color_theme))),
            space: Points(b.space),
            shadow: b.shadow,
        })
    }
}

/// The model's line for a border style.
pub(crate) fn border_style(s: BorderStyle) -> Line {
    match s {
        BorderStyle::Single => Line::Single,
        BorderStyle::Double => Line::Double,
        BorderStyle::Dotted => Line::Dotted,
        BorderStyle::Dashed => Line::Dashed,
        BorderStyle::Thick => Line::Thick,
        BorderStyle::DotDash => Line::DotDash,
        BorderStyle::DotDotDash => Line::DotDotDash,
        BorderStyle::Triple => Line::Triple,
        BorderStyle::ThinThickSmallGap => Line::ThinThickSmallGap,
        BorderStyle::ThickThinSmallGap => Line::ThickThinSmallGap,
        BorderStyle::ThinThickThinSmallGap => Line::ThinThickThinSmallGap,
        BorderStyle::Wave => Line::Wave,
        BorderStyle::DoubleWave => Line::DoubleWave,
        BorderStyle::ThreeDEmboss => Line::ThreeDEmboss,
        BorderStyle::ThreeDEngrave => Line::ThreeDEngrave,
        BorderStyle::Outset => Line::Outset,
        BorderStyle::Inset => Line::Inset,
        BorderStyle::DashSmallGap => Line::DashSmallGap,
        BorderStyle::DashDotStroked => Line::DashDotStroked,
        BorderStyle::ThinThickMediumGap => Line::ThinThickMediumGap,
        BorderStyle::ThickThinMediumGap => Line::ThickThinMediumGap,
        BorderStyle::ThinThickThinMediumGap => Line::ThinThickThinMediumGap,
        BorderStyle::ThinThickLargeGap => Line::ThinThickLargeGap,
        BorderStyle::ThickThinLargeGap => Line::ThickThinLargeGap,
        BorderStyle::ThinThickThinLargeGap => Line::ThinThickThinLargeGap,
    }
}

/// The border style a model line reads back as: single for the ones not offered (none, nil and art borders).
pub(crate) fn api_border_style(s: Line) -> BorderStyle {
    match s {
        Line::Single => BorderStyle::Single,
        Line::Double => BorderStyle::Double,
        Line::Dotted => BorderStyle::Dotted,
        Line::Dashed => BorderStyle::Dashed,
        Line::Thick => BorderStyle::Thick,
        Line::DotDash => BorderStyle::DotDash,
        Line::DotDotDash => BorderStyle::DotDotDash,
        Line::Triple => BorderStyle::Triple,
        Line::ThinThickSmallGap => BorderStyle::ThinThickSmallGap,
        Line::ThickThinSmallGap => BorderStyle::ThickThinSmallGap,
        Line::ThinThickThinSmallGap => BorderStyle::ThinThickThinSmallGap,
        Line::Wave => BorderStyle::Wave,
        Line::DoubleWave => BorderStyle::DoubleWave,
        Line::ThreeDEmboss => BorderStyle::ThreeDEmboss,
        Line::ThreeDEngrave => BorderStyle::ThreeDEngrave,
        Line::Outset => BorderStyle::Outset,
        Line::Inset => BorderStyle::Inset,
        Line::DashSmallGap => BorderStyle::DashSmallGap,
        Line::DashDotStroked => BorderStyle::DashDotStroked,
        Line::ThinThickMediumGap => BorderStyle::ThinThickMediumGap,
        Line::ThickThinMediumGap => BorderStyle::ThickThinMediumGap,
        Line::ThinThickThinMediumGap => BorderStyle::ThinThickThinMediumGap,
        Line::ThinThickLargeGap => BorderStyle::ThinThickLargeGap,
        Line::ThickThinLargeGap => BorderStyle::ThickThinLargeGap,
        Line::ThinThickThinLargeGap => BorderStyle::ThinThickThinLargeGap,
        Line::None | Line::Nil | Line::Art(_) => BorderStyle::Single,
    }
}

/// A border element: `<w:top w:val w:sz w:space w:color [theme] w:shadow/>`, its colour `c` (none: automatic).
pub fn element(w: &str, name: &str, style: Line, width: f64, space: f64, shadow: bool, c: Option<WordColour>) -> String {
    let tag = format!(
        "<{w}{name} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"{}\"/>",
        crate::docx::line_style::border_val(style),
        (width * 8.0).round() as i64,
        space.round() as i64
    );
    let tag = super::commands::colour_attrs(&tag, w, "color", super::commands::THEME_COLOR, c);
    if shadow { super::commands::with_attr(&tag, &format!("{w}shadow"), "1") } else { tag }
}

/// A model border's element, its colour as written.
pub fn border_element(w: &str, name: &str, b: &Border) -> String {
    element(w, name, b.style, b.width, b.space, b.shadow, b.color.map(|rgb| WordColour { rgb, theme: b.color_theme }))
}

/// A paragraph's border edges to set: each set, removed (`Some(None)`) or kept (`None`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParagraphBorderEdges {
    /// The top edge.
    pub top: Option<Option<BorderLine>>,
    /// The left edge.
    pub left: Option<Option<BorderLine>>,
    /// The bottom edge.
    pub bottom: Option<Option<BorderLine>>,
    /// The right edge.
    pub right: Option<Option<BorderLine>>,
    /// The line between paragraphs with the same borders.
    pub between: Option<Option<BorderLine>>,
}

/// Shading: a fill, a pattern of the pattern colour over it, and that colour.
pub use crate::api::state::Shading as ShadingValue;

impl ShadingValue {
    /// A model shading as read back; none for no shading (`nil`, or a clear pattern without a fill).
    pub fn of(s: &Shd) -> Option<ShadingValue> {
        let pattern = ShadingPattern::of_val(&crate::docx::line_style::shading_val(s.pattern));
        if pattern == ShadingPattern::Nil || (pattern == ShadingPattern::Clear && s.fill.is_none()) {
            return None;
        }
        Some(ShadingValue {
            fill: s.fill.map_or(NoneOr::None, |c| NoneOr::Value(colour_value(c, s.fill_theme))),
            pattern,
            color: s.color.map_or(AutoOr::Auto, |c| AutoOr::Value(colour_value(c, s.color_theme))),
        })
    }
}

/// A colour given as RGB, as RGB; any other is automatic.
pub fn rgb_only(c: &AutoOr<ColourValue>) -> AutoOr<crate::api::value::Color> {
    match c {
        AutoOr::Value(ColourValue { rgb: Some(c), .. }) => AutoOr::Value(*c),
        _ => AutoOr::Auto,
    }
}

/// A colour as read back: none for automatic.
pub fn colour_of(rgb: Option<u32>, theme: Option<crate::docx::ThemeColourRef>) -> AutoOr<ColourValue> {
    rgb.map_or(AutoOr::Auto, |c| AutoOr::Value(colour_value(c, theme)))
}

/// `b` checked, and its colour in `e`'s document.
pub(super) fn checked(e: &super::Editor, b: &BorderLine) -> Result<Option<WordColour>, String> {
    b.check()?;
    word_colour_or_auto(e, &b.color)
}

/// A WordprocessingML colour from a colour value or automatic, `None` for automatic.
pub(super) fn word_colour_or_auto(e: &super::Editor, c: &AutoOr<ColourValue>) -> Result<Option<WordColour>, String> {
    match c {
        AutoOr::Auto => Ok(None),
        AutoOr::Value(c) => e.word_colour(c).map(Some),
    }
}

/// The selection's character formatting beyond [`crate::api::state::CharacterFormatting`]'s, each the same over the
/// selection or mixed.
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterReadback {
    /// The theme font the text's font is (none: a named font).
    pub font_theme: Tri<NoneOr<ThemeFont>>,
    /// The text's colour as written: RGB or a theme colour with its tint or shade.
    pub color: Tri<AutoOr<ColourValue>>,
    /// The underline's colour as written (automatic: the text's).
    pub underline_color: Tri<AutoOr<ColourValue>>,
    /// The text's border.
    pub border: Tri<NoneOr<BorderLine>>,
    /// The text's shading.
    pub shading: Tri<NoneOr<ShadingValue>>,
    /// The width the text is fitted to (Fit Text).
    pub fit_text: Tri<NoneOr<Points>>,
    /// The OpenType ligatures, each kind on or off.
    pub ligatures: Tri<LigatureFlags>,
    /// The stylistic sets on.
    pub stylistic_sets: Tri<StylisticSets>,
}

/// A set of OpenType stylistic sets, 1 to 20.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StylisticSets(pub u32);

impl StylisticSets {
    /// The sets on, in order.
    pub fn ids(self) -> impl Iterator<Item = u8> {
        (1..=20u8).filter(move |n| self.0 & 1 << (n - 1) != 0)
    }
}

impl CharacterReadback {
    /// Into the formatting state's fields.
    pub fn fill(self, f: &mut crate::api::state::CharacterFormatting) {
        f.font_theme = self.font_theme.map(|t| match t {
            NoneOr::None => NoneOr::None,
            NoneOr::Value(ThemeFont::Major) => NoneOr::Value(crate::api::value::ThemeFont::Headings),
            NoneOr::Value(ThemeFont::Minor) => NoneOr::Value(crate::api::value::ThemeFont::Body),
        });
        f.color_value = self.color;
        f.underline_color_value = self.underline_color;
        f.text_border = self.border.map(api_border);
        f.text_shading = self.shading;
        f.fit_text = self.fit_text;
        f.ligature_flags = self.ligatures;
        f.stylistic_sets = self.stylistic_sets.map(|s| s.ids().collect());
    }
}

/// A border as the API reads it back.
fn api_border(b: NoneOr<BorderLine>) -> NoneOr<crate::api::command::Border> {
    match b {
        NoneOr::None => NoneOr::None,
        NoneOr::Value(b) => NoneOr::Value(b.into()),
    }
}

impl ParagraphReadback {
    /// Into the formatting state's fields.
    pub fn fill(self, f: &mut crate::api::state::ParagraphFormatting) {
        f.border_top = self.border_top.map(api_border);
        f.border_left = self.border_left.map(api_border);
        f.border_bottom = self.border_bottom.map(api_border);
        f.border_right = self.border_right.map(api_border);
        f.border_between = self.border_between.map(api_border);
        f.shading_value = self.shading;
        f.tabs = self.tabs;
    }
}

/// [`CharacterReadback`] over `runs`.
pub fn character(runs: &[&RunStyle]) -> CharacterReadback {
    CharacterReadback {
        font_theme: fold(runs.iter().map(|r| r.font_theme.map_or(NoneOr::None, NoneOr::Value))),
        color: fold(runs.iter().map(|r| colour_of(r.color, r.color_theme))),
        underline_color: fold(runs.iter().map(|r| colour_of(r.underline_color, r.underline_color_theme))),
        border: fold(runs.iter().map(|r| r.border.as_ref().and_then(BorderLine::of).map_or(NoneOr::None, NoneOr::Value))),
        shading: fold(runs.iter().map(|r| r.shd.as_ref().and_then(ShadingValue::of).map_or(NoneOr::None, NoneOr::Value))),
        fit_text: fold(runs.iter().map(|r| r.fit_text.map_or(NoneOr::None, |f| NoneOr::Value(Points(f.0))))),
        ligatures: fold(runs.iter().map(|r| LigatureFlags::of(r.typography.ligatures))),
        stylistic_sets: fold(runs.iter().map(|r| StylisticSets(r.typography.stylistic_sets & 0xF_FFFF))),
    }
}

/// The selected paragraphs' formatting beyond [`crate::api::state::ParagraphFormatting`]'s, each the same over them or
/// mixed.
#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphReadback {
    /// The top edge.
    pub border_top: Tri<NoneOr<BorderLine>>,
    /// The left edge.
    pub border_left: Tri<NoneOr<BorderLine>>,
    /// The bottom edge.
    pub border_bottom: Tri<NoneOr<BorderLine>>,
    /// The right edge.
    pub border_right: Tri<NoneOr<BorderLine>>,
    /// The line between paragraphs with the same borders.
    pub border_between: Tri<NoneOr<BorderLine>>,
    /// The shading.
    pub shading: Tri<NoneOr<ShadingValue>>,
    /// The tab stops, in position order.
    pub tabs: Tri<Vec<crate::api::services::TabStopInfo>>,
}

/// [`ParagraphReadback`] over `paras` and their tab stops `tabs`.
pub fn paragraph(paras: &[&ParaStyle], tabs: Tri<Vec<crate::api::services::TabStopInfo>>) -> ParagraphReadback {
    let edge = |f: &dyn Fn(&ParaStyle) -> &Option<Border>| {
        fold(paras.iter().map(|p| f(p).as_ref().and_then(BorderLine::of).map_or(NoneOr::None, NoneOr::Value)))
    };
    ParagraphReadback {
        border_top: edge(&|p| &p.border_top),
        border_left: edge(&|p| &p.border_left),
        border_bottom: edge(&|p| &p.border_bottom),
        border_right: edge(&|p| &p.border_right),
        border_between: edge(&|p| &p.border_between),
        shading: fold(paras.iter().map(|p| p.shd.as_ref().and_then(ShadingValue::of).map_or(NoneOr::None, NoneOr::Value))),
        tabs,
    }
}

impl super::Editor {
    /// The selection's character formatting beyond the formatting state's ([`CharacterReadback`]).
    pub fn character_readback(&mut self) -> Option<CharacterReadback> {
        let s = self.selection_sources()?;
        Some(self.character_of(&s))
    }

    /// [`character`] of the selection's runs; with list numbers selected, of the number, as the formatting state reads it.
    fn character_of(&self, s: &super::state::SelectionSources) -> CharacterReadback {
        match self.list_label_selection().and_then(|l| self.index.para(&self.doc, l.para)).and_then(|p| p.list.as_ref()) {
            Some(l) => character(&[&*l.run]),
            None => character(&s.runs.iter().map(|r| &**r).collect::<Vec<_>>()),
        }
    }

    /// The selected paragraphs' formatting beyond the formatting state's ([`ParagraphReadback`]). The tab stops are
    /// the first paragraph's when every paragraph has the same, and mixed otherwise.
    pub fn paragraph_readback(&mut self) -> Option<ParagraphReadback> {
        let s = self.selection_sources()?;
        self.paragraph_readback_of(&s)
    }

    /// The formatting state and both read-backs, the selection walked once.
    pub fn format_readback(&mut self) -> Option<(super::state::FormatState, CharacterReadback, ParagraphReadback)> {
        let s = self.selection_sources()?;
        let state = self.format_state_of(&s)?;
        let paragraph = self.paragraph_readback_of(&s)?;
        Some((state, self.character_of(&s), paragraph))
    }

    fn paragraph_readback_of(&mut self, s: &super::state::SelectionSources) -> Option<ParagraphReadback> {
        let mut styles: Vec<(std::rc::Rc<ParaStyle>, crate::layout::ParaId)> =
            s.paras.iter().filter_map(|&p| Some((self.index.para(&self.doc, p)?.style.clone(), p))).collect();
        styles.dedup_by(|a, b| std::rc::Rc::ptr_eq(&a.0, &b.0));
        // The stops as they read, which they inherit included
        let first = self.tab_stops(styles.first()?.1);
        let tabs = if styles[1..].iter().all(|(_, p)| self.tab_stops(*p) == first) { Tri::Uniform(first) } else { Tri::Mixed };
        Some(paragraph(&styles.iter().map(|(p, _)| &**p).collect::<Vec<_>>(), tabs))
    }
}
