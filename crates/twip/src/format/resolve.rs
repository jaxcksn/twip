//! Resolution on typed levels: the effective format of runs and paragraphs

use std::collections::HashMap;
use std::rc::Rc;

use super::{CharProps, Color, FontHint, FontRef, ParaFlag, ParaProps, ScriptClass, TextBorder, ThemeFace, TightWrap, Toggle};
use crate::document::format::{Emphasis, Jc, LineRule, ShadingPattern, VertAlign};
use crate::document::line::{BorderStyle, Underline};
use crate::document::paragraph::{ListJc, TextAlignment};
use crate::docx::font_table::GenericFamily;
use crate::docx::model::{ParaStyle, RunStyle, TextEffects, ThemeColourRef, ThemeFont, Typography};
use crate::docx::props::{Border, Shd, ThemeColours};
use crate::docx::styles::ThemeFonts;

/// What resolving a run needs from its document.
#[derive(Clone, Copy)]
pub struct RunEnv<'a> {
    /// The theme's colours.
    pub colours: &'a ThemeColours,
    /// The theme's fonts.
    pub fonts: &'a ThemeFonts,
    /// The generic category of each family the document describes, by lowercase name.
    pub categories: &'a HashMap<String, GenericFamily>,
    /// OpenType features apply.
    pub open_type: bool,
}

/// A run's levels over its paragraph's `base`: its character style chain's fold (a toggle level), then its direct
/// formatting.
pub fn run_props(base: &CharProps, character_style: Option<&CharProps>, direct: &CharProps) -> CharProps {
    let mut p = base.clone();
    if let Some(cs) = character_style {
        p.merge_level(cs);
    }
    p.merge(direct);
    p
}

/// The family a theme font names.
fn theme_family(fonts: &ThemeFonts, face: ThemeFace, script: ScriptClass) -> String {
    let (latin, ea, cs) = match face {
        ThemeFace::Major => (&fonts.major, &fonts.major_ea, &fonts.major_cs),
        ThemeFace::Minor | ThemeFace::Unrecognised => (&fonts.minor, &fonts.minor_ea, &fonts.minor_cs),
    };
    match script {
        ScriptClass::EastAsian => ea.as_ref().unwrap_or(latin).clone(),
        ScriptClass::Complex => cs.as_ref().unwrap_or(latin).clone(),
        ScriptClass::Latin | ScriptClass::LatinExtended => latin.clone(),
    }
}

/// An East Asian theme font whose slot has no typeface.
fn east_asian_unset(fonts: &ThemeFonts, f: &FontRef) -> bool {
    match f {
        FontRef::Theme { face: ThemeFace::Major, script: ScriptClass::EastAsian } => fonts.major_ea.is_none(),
        FontRef::Theme { face: ThemeFace::Minor | ThemeFace::Unrecognised, script: ScriptClass::EastAsian } => fonts.minor_ea.is_none(),
        _ => false,
    }
}

/// The East Asian family a level's East Asian font `f` names: none for an East Asian theme font whose slot has no
/// typeface (empty).
pub fn default_east_asian(fonts: &ThemeFonts, f: &Option<FontRef>) -> Option<String> {
    match f {
        Some(t) if east_asian_unset(fonts, t) => Some(String::new()),
        f => family(fonts, f),
    }
}

fn family(fonts: &ThemeFonts, f: &Option<FontRef>) -> Option<String> {
    match f {
        Some(FontRef::Theme { face, script }) => Some(theme_family(fonts, *face, *script)),
        Some(FontRef::Family(n)) => Some(n.to_string()),
        None => None,
    }
}

/// An authored colour as shown, with the theme colour it is.
fn colour(env: &RunEnv, c: Option<Color>) -> (Option<u32>, Option<ThemeColourRef>) {
    match c {
        Some(Color::Rgb(v)) => (Some(v), None),
        Some(Color::Theme(r)) => (Some(env.colours.resolve(r)), Some(r)),
        Some(Color::Auto) | None => (None, None),
    }
}

/// The effective format of a run whose folded levels are `p`.
pub fn run_style(env: &RunEnv, p: &CharProps) -> RunStyle {
    let fonts = env.fonts;
    let latin = family(fonts, &p.fonts.latin);
    let latin_extended = family(fonts, &p.fonts.latin_extended);
    let fallback = || latin.clone().or_else(|| latin_extended.clone()).unwrap_or_else(|| fonts.minor.clone());
    let east_asian = match (&p.fonts.east_asian, &p.east_asian_below) {
        (Some(t @ FontRef::Theme { .. }), Some(below)) if east_asian_unset(fonts, t) => family(fonts, &Some(below.clone())),
        (e, _) => family(fonts, e),
    };
    let ea_theme = p.fonts.east_asian.as_ref().filter(|e| matches!(e, FontRef::Theme { .. })).or(p.east_asian_theme.as_ref());
    let east_asia_unset = p.east_asian_below.is_none() && ea_theme.is_some_and(|t| matches!(t, FontRef::Theme { .. }) && east_asian_unset(fonts, t));
    let cs = family(fonts, &p.fonts.complex).unwrap_or_else(fallback);
    let complex = p.complex == Some(true);
    let ascii = if complex { cs.clone() } else { latin.clone().or_else(|| latin_extended.clone()).unwrap_or_else(fallback) };
    let h_ansi = if complex { cs.clone() } else { latin_extended.clone().or_else(|| latin.clone()).unwrap_or_else(fallback) };
    let east_asia = east_asian.clone().unwrap_or_else(fallback);
    let category = |f: &str| env.categories.get(&f.to_lowercase()).copied();
    let categories = if env.categories.is_empty() { [None; 3] } else { [category(&ascii), category(&h_ansi), category(&east_asia)] };
    let toggle = |t: Toggle| p.toggles.is_on(t);
    let (underline_color, underline_color_theme) = colour(env, p.underline.and_then(|u| u.color));
    let (color, color_theme) = colour(env, p.color);
    let font_theme = match if complex {
        p.fonts.complex.as_ref().or(p.fonts.latin.as_ref()).or(p.fonts.latin_extended.as_ref())
    } else {
        p.fonts.latin.as_ref().or(p.fonts.latin_extended.as_ref())
    } {
        Some(FontRef::Theme { face: ThemeFace::Major, .. }) => Some(ThemeFont::Major),
        Some(FontRef::Theme { face: ThemeFace::Minor, .. }) => Some(ThemeFont::Minor),
        _ => None,
    };
    let shd = p.shading.map(|s| {
        let (color, color_theme) = colour(env, s.color);
        let (fill, fill_theme) = colour(env, s.fill);
        Shd { pattern: s.pattern, color, fill, color_theme, fill_theme }
    });
    let border = p.border.map(|b| {
        let (color, color_theme) = colour(env, b.color);
        Border { style: b.style, width: b.width, space: b.space, shadow: b.shadow, color, color_theme }
    });
    let ty = &p.typography;
    let ligatures = ty.ligatures.unwrap_or(0);
    let contextual = ty.contextual_alternates == Some(true);
    let number_form = ty.number_form.map_or(0, |n| n.bits());
    let number_spacing = ty.number_spacing.map_or(0, |n| n.bits());
    let sets = ty.stylistic_sets.unwrap_or(0);
    RunStyle {
        east_asia_named: !east_asia_unset && east_asian.as_deref().is_some_and(|n| !n.is_empty() && !n.eq_ignore_ascii_case("Times New Roman")),
        ascii,
        h_ansi,
        east_asia,
        cs,
        east_asia_unset,
        hint_east_asia: p.hint == Some(FontHint::EastAsia),
        size: if complex { p.complex_size } else { p.size }.unwrap_or(10.0),
        bold: toggle(if complex { Toggle::BOLD_COMPLEX } else { Toggle::BOLD }),
        italic: toggle(if complex { Toggle::ITALIC_COMPLEX } else { Toggle::ITALIC }),
        caps: toggle(Toggle::CAPS),
        small_caps: toggle(Toggle::SMALL_CAPS),
        strike: toggle(Toggle::STRIKE),
        dstrike: p.double_strike.unwrap_or(false),
        hidden: toggle(Toggle::HIDDEN),
        no_proof: p.no_proof.unwrap_or(false),
        underline: p.underline.map(|u| u.style).filter(|u| *u != Underline::None),
        underline_color,
        underline_color_theme,
        emboss: toggle(Toggle::EMBOSS),
        imprint: toggle(Toggle::IMPRINT),
        outline: toggle(Toggle::OUTLINE),
        shadow: toggle(Toggle::SHADOW),
        typography: Typography { ligatures, contextual_alternates: contextual, number_form, number_spacing, stylistic_sets: sets },
        effects: p.effects.as_ref().and_then(TextEffects::shown),
        color,
        color_theme,
        font_theme,
        highlight: p.highlight.flatten().map(|h| h.rgb()),
        shading: shd.as_ref().and_then(Shd::flat),
        shd: shd.filter(|s| s.pattern != ShadingPattern::Nil),
        border: border.filter(|b| b.visible() || b.style == BorderStyle::None && b.space > 0.0),
        vert_align: p.vertical.unwrap_or(VertAlign::Baseline),
        position: p.position.unwrap_or(0.0),
        spacing: p.spacing.unwrap_or(0.0),
        scale: p.scale.unwrap_or(100.0),
        kern: p.kern.unwrap_or(0.0),
        ligatures: if env.open_type { ligatures | if contextual { 16 } else { 0 } } else { 0 },
        figures: if env.open_type { number_form | number_spacing | sets << 4 } else { 0 },
        fit_text: p.fit_text,
        em: p.emphasis.filter(|e| *e != Emphasis::None),
        toc: false,
        lang: p.lang.latin.as_deref().unwrap_or_default().to_string(),
        lang_east_asia: p.lang.east_asian.as_deref().unwrap_or_default().to_string(),
        style_id: p.style.as_deref().map(str::to_string),
        direct: Default::default(),
        rev: Default::default(),
        categories,
    }
}

/// The effective format of a paragraph whose folded levels are `p`, before its list, its run base and its divisions;
/// a theme colour as `colours` resolve it.
pub fn para_style(colours: &ThemeColours, p: &ParaProps) -> ParaStyle {
    let line_rule = p.line_rule.unwrap_or(LineRule::Auto);
    let line = match (p.line, line_rule) {
        (Some(l), LineRule::Auto) => l / 240.0,
        (Some(l), _) => l / 20.0,
        (None, _) => 1.0,
    };
    let env = |c| para_colour(colours, c);
    let bdr = |b: &Option<TextBorder>| {
        b.filter(|b| !matches!(b.style, BorderStyle::None | BorderStyle::Nil)).map(|b| {
            let (color, color_theme) = env(b.color);
            Border { style: b.style, width: b.width, space: b.space, shadow: b.shadow, color, color_theme }
        })
    };
    let shd = p.shading.map(|s| {
        let (color, color_theme) = env(s.color);
        let (fill, fill_theme) = env(s.fill);
        Shd { pattern: s.pattern, color, fill, color_theme, fill_theme }
    });
    let flag = |f: ParaFlag| p.flags.get(f);
    ParaStyle {
        style_id: p.style.as_deref().map(str::to_string),
        jc: p.align.unwrap_or(Jc::Left),
        bidi: flag(ParaFlag::BIDI).unwrap_or(false),
        ind_left: p.indent_left.unwrap_or(0.0),
        ind_right: p.indent_right.unwrap_or(0.0),
        ind_first: p.indent_first.unwrap_or(0.0),
        space_before: p.space_before.unwrap_or(0.0),
        space_after: p.space_after.unwrap_or(0.0),
        before_auto: p.space_before_auto.unwrap_or(false),
        after_auto: p.space_after_auto.unwrap_or(false),
        line,
        line_rule: if p.line.is_some() { line_rule } else { LineRule::Auto },
        contextual: flag(ParaFlag::CONTEXTUAL).unwrap_or(false),
        snap_to_grid: flag(ParaFlag::SNAP_TO_GRID).unwrap_or(true),
        keep_next: flag(ParaFlag::KEEP_NEXT).unwrap_or(false),
        keep_lines: flag(ParaFlag::KEEP_LINES).unwrap_or(false),
        page_break_before: flag(ParaFlag::PAGE_BREAK_BEFORE).unwrap_or(false),
        widow_control: flag(ParaFlag::WIDOW_CONTROL).unwrap_or(true),
        suppress_auto_hyphens: flag(ParaFlag::SUPPRESS_AUTO_HYPHENS).unwrap_or(false),
        no_kinsoku: flag(ParaFlag::KINSOKU) == Some(false),
        no_overflow_punct: flag(ParaFlag::OVERFLOW_PUNCT) == Some(false),
        no_auto_space_de: flag(ParaFlag::AUTO_SPACE_DE) == Some(false),
        no_auto_space_dn: flag(ParaFlag::AUTO_SPACE_DN) == Some(false),
        top_line_punct: flag(ParaFlag::TOP_LINE_PUNCT).unwrap_or(false),
        no_word_wrap: flag(ParaFlag::WORD_WRAP) == Some(false),
        tight_wrap: p.tight_wrap.map_or(0, TightWrap::code),
        suppress_overlap: flag(ParaFlag::SUPPRESS_OVERLAP).unwrap_or(false),
        suppress_line_numbers: flag(ParaFlag::SUPPRESS_LINE_NUMBERS).unwrap_or(false),
        mirror_indents: flag(ParaFlag::MIRROR_INDENTS).unwrap_or(false),
        div: p.division,
        div_boxes: Rc::from([]),
        tabs: p.resolved_tabs(),
        border_top: bdr(&p.borders.top),
        border_bottom: bdr(&p.borders.bottom),
        border_left: bdr(&p.borders.left),
        border_right: bdr(&p.borders.right),
        border_between: bdr(&p.borders.between),
        shading: shd.as_ref().and_then(Shd::flat),
        shd: shd.filter(|s| s.pattern != ShadingPattern::Nil),
        frame: p.frame.clone(),
        text_flow: p.text_flow.unwrap_or(0),
        in_frame: false,
        text_alignment: p.text_alignment.unwrap_or(TextAlignment::Auto),
        outline_lvl: p.outline_level,
        collapsed: p.collapsed.unwrap_or(false),
        list_jc: ListJc::Left,
        num: None,
        list_fmt: None,
        list: None,
        mark_size: None,
        direct: Default::default(),
        change: None,
        joined: Rc::from([]),
        parts: Rc::from([]),
    }
}

/// An authored colour as shown, with the theme colour it is.
fn para_colour(colours: &ThemeColours, c: Option<Color>) -> (Option<u32>, Option<ThemeColourRef>) {
    match c {
        Some(Color::Rgb(v)) => (Some(v), None),
        Some(Color::Theme(r)) => (Some(colours.resolve(r)), Some(r)),
        Some(Color::Auto) | None => (None, None),
    }
}

// Paragraphs joined through vanished marks

/// The most paragraphs a paragraph's [`ParaStyle::parts`] keep.
pub const MAX_PARTS: usize = 256;

/// The style of `later` joined onto `first`, the paragraph before it (`at` units long) whose mark is vanished: the
/// first one's indents, alignment, tabs and space before (unless it is empty), the first one's borders and shading,
/// the last one's other properties. An empty first paragraph gives nothing but a style separator's (`spec`, in compat
/// 15) alignment and indents. In compat ≤ 14 (`legacy`) each later line takes its indents, alignment, tabs, borders and
/// shading from the paragraph of its first character, so `joined` keeps the later one's own.
pub fn joined_style(first: &ParaStyle, later: &Rc<ParaStyle>, at: u32, spec: Option<&ParaStyle>, legacy: bool) -> ParaStyle {
    let mut style = (**later).clone();
    style.parts = match first.parts.len() {
        // Parts, at most MAX_PARTS
        MAX_PARTS.. => first.parts.clone(),
        0 => Rc::from([Rc::new(first.clone()), later.clone()]),
        _ => first.parts.iter().cloned().chain([later.clone()]).collect(),
    };
    if let (0, Some(s)) = (at, spec) {
        (style.ind_left, style.ind_right, style.ind_first, style.jc) = (s.ind_left, s.ind_right, s.ind_first, s.jc);
    }
    if at > 0 {
        (style.ind_left, style.ind_right, style.ind_first, style.jc) = (first.ind_left, first.ind_right, first.ind_first, first.jc);
        (style.space_before, style.before_auto) = (first.space_before, first.before_auto);
        style.tabs = first.tabs.clone();
        if legacy {
            style.joined = first.joined.iter().cloned().chain([(at, later.clone())]).collect();
        }
        (style.border_top, style.border_bottom, style.border_left, style.border_right, style.border_between) = (
            first.border_top.clone(),
            first.border_bottom.clone(),
            first.border_left.clone(),
            first.border_right.clone(),
            first.border_between.clone(),
        );
        (style.shading, style.shd) = (first.shading, first.shd.clone());
    }
    style
}
