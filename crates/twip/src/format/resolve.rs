//! Run resolution on typed levels: folding, toggles, and the effective format

use std::collections::HashMap;
use std::rc::Rc;

use super::{CharProps, Color, FontHint, FontRef, ScriptClass, ThemeFace, Toggle};
use crate::document::format::{Emphasis, ShadingPattern, VertAlign};
use crate::document::line::{BorderStyle, Underline};
use crate::docx::font_table::GenericFamily;
use crate::docx::model::{RunStyle, TextEffects, ThemeColourRef, ThemeFont, Typography};
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

impl CharProps {
    /// Overrides everything `o` sets; toggles are absolute (direct formatting, or within one style chain).
    /// `foreign` stays this level's.
    pub fn merge(&mut self, o: &CharProps) {
        let family = |f: &Option<FontRef>| matches!(f, Some(FontRef::Family(_)));
        let theme = |f: &Option<FontRef>| matches!(f, Some(FontRef::Theme { .. }));
        let (mine, theirs) = (&self.fonts.east_asian, &o.fonts.east_asian);
        let named = family(mine) || self.east_asian_below.is_some();
        if theme(mine) && family(theirs) && self.east_asian_below.is_none() {
            self.east_asian_theme = mine.clone();
        } else if theirs.is_some() {
            self.east_asian_theme = None;
        }
        if o.east_asian_theme.is_some() && !named {
            self.east_asian_theme = o.east_asian_theme.clone();
        }
        let mine = &self.fonts.east_asian;
        if family(mine) && theme(theirs) {
            self.east_asian_below = mine.clone();
        } else if family(theirs) {
            self.east_asian_below = None;
        }
        if o.east_asian_below.is_some() {
            self.east_asian_below = o.east_asian_below.clone();
        }
        macro_rules! take {
            ($($f:ident).+) => { if o.$($f).+.is_some() { self.$($f).+ = o.$($f).+.clone(); } };
        }
        take!(fonts.latin);
        take!(fonts.latin_extended);
        take!(fonts.east_asian);
        take!(fonts.complex);
        take!(hint);
        take!(size);
        take!(complex_size);
        take!(complex);
        take!(double_strike);
        take!(no_proof);
        take!(underline);
        take!(color);
        take!(highlight);
        take!(shading);
        take!(border);
        take!(vertical);
        take!(position);
        take!(spacing);
        take!(scale);
        take!(kern);
        take!(typography.ligatures);
        take!(typography.contextual_alternates);
        take!(typography.number_form);
        take!(typography.number_spacing);
        take!(typography.stylistic_sets);
        take!(lang.latin);
        take!(lang.east_asian);
        take!(lang.complex);
        take!(style);
        take!(rtl);
        take!(emphasis);
        take!(fit_text);
        match (&mut self.effects, &o.effects) {
            (_, None) => {}
            (None, Some(e)) => self.effects = Some(e.clone()),
            (Some(mine), Some(e)) => Rc::make_mut(mine).merge(e),
        }
        let t = &mut self.toggles;
        t.on = (t.on & !o.toggles.set) | (o.toggles.on & o.toggles.set);
        t.set |= o.toggles.set;
    }

    /// Adds a style chain's level: its values override, and each toggle it turns on flips the one below.
    pub fn merge_level(&mut self, level: &CharProps) {
        let flip = level.toggles.on & level.toggles.set;
        let toggles = self.toggles;
        self.merge(level);
        self.toggles.on = toggles.on ^ flip;
        self.toggles.set = toggles.set | flip;
    }

    /// One style chain's levels, root first, folded: the nearest level decides each toggle, and only toggles that
    /// end on count.
    pub fn fold_chain<'a>(chain: impl IntoIterator<Item = &'a CharProps>) -> CharProps {
        let mut acc = CharProps::default();
        for level in chain {
            acc.merge(level);
        }
        let on = acc.toggles.on & acc.toggles.set;
        acc.toggles.on = on;
        acc.toggles.set = on;
        acc
    }
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
