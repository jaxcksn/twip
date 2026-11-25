//! Character formatting

use crate::api::command::{CaseChange, CharacterEffects, Language, Ligatures, NumberForm, NumberSpacing, Typography};
use crate::api::drawing::{ColourValue, TextEffectsChange};
use crate::api::value::{AutoOr, Color, HighlightColor, Points, StyleId, UnderlineStyle};
use crate::docx::Inline;
use crate::docx::write::props::Props;
use crate::docx::{RunStyle, ThemeFont, Underline};
use crate::font::Fonts;
use crate::format::{CharEdit, FontRef, Prop, ScriptClass, ThemeFace, UnderlineProps};
use crate::layout::Unit;

use super::Outcome;
use crate::docx::run_xml;
use crate::editing::format::{Format, Toggle};
use crate::editing::step::Step;
use crate::editing::{Editor, Tx};

pub(crate) use crate::docx::char_props::with_attr;

pub use crate::api::command::LigatureFlags;

impl LigatureFlags {
    /// As `w14:ligatures` bits.
    pub fn bits(self) -> u8 {
        u8::from(self.standard) | u8::from(self.contextual) << 1 | u8::from(self.historical) << 2 | u8::from(self.discretionary) << 3
    }

    /// From `w14:ligatures` bits.
    pub fn of(bits: u8) -> LigatureFlags {
        LigatureFlags { standard: bits & 1 != 0, contextual: bits & 2 != 0, historical: bits & 4 != 0, discretionary: bits & 8 != 0 }
    }
}

/// OpenType settings beyond [`Typography`]'s: each set, or kept (`None`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenTypeChange {
    /// The ligatures, each kind on or off; over `Typography::ligatures`.
    pub ligatures: Option<LigatureFlags>,
    /// The stylistic sets on, each 1 to 20 (none: off); over `Typography::stylistic_set`.
    pub stylistic_sets: Option<Vec<u8>>,
}

/// The attributes a colour's theme colour, tint and shade are written in.
pub(crate) const THEME_COLOR: [&str; 3] = ["themeColor", "themeTint", "themeShade"];
/// The attributes a shading fill's theme colour, tint and shade are written in.
pub(crate) const THEME_FILL: [&str; 3] = ["themeFill", "themeFillTint", "themeFillShade"];

/// `tag` with colour `c` (none: `auto`) in its attribute `hex` and its theme colour in `names` (each of which `tag` is
/// without).
pub(crate) fn colour_attrs(tag: &str, w: &str, hex: &str, names: [&str; 3], c: Option<crate::editing::theme::WordColour>) -> String {
    let Some(c) = c else { return with_attr(tag, &format!("{w}{hex}"), "auto") };
    let mut t = with_attr(tag, &format!("{w}{hex}"), &format!("{:06X}", c.rgb));
    if let Some(r) = c.theme
        && let Some(name) = crate::docx::props::theme_name(r.slot)
    {
        t = with_attr(&t, &format!("{w}{}", names[0]), name);
        for (b, n) in [(r.tint, names[1]), (r.shade, names[2])] {
            if let Some(b) = b {
                t = with_attr(&t, &format!("{w}{n}"), &format!("{b:02X}"));
            }
        }
    }
    t
}

/// The underline an underline style draws.
pub(crate) fn underline(style: UnderlineStyle) -> Underline {
    match style {
        UnderlineStyle::None => Underline::None,
        UnderlineStyle::Single => Underline::Single,
        UnderlineStyle::Words => Underline::Words,
        UnderlineStyle::Double => Underline::Double,
        UnderlineStyle::Thick => Underline::Thick,
        UnderlineStyle::Dotted => Underline::Dotted,
        UnderlineStyle::Dash => Underline::Dash,
        UnderlineStyle::DotDash => Underline::DotDash,
        UnderlineStyle::Wave => Underline::Wave,
        UnderlineStyle::DottedHeavy => Underline::DottedHeavy,
        UnderlineStyle::DashedHeavy => Underline::DashedHeavy,
        UnderlineStyle::DashLong => Underline::DashLong,
        UnderlineStyle::DashLongHeavy => Underline::DashLongHeavy,
        UnderlineStyle::DashDotHeavy => Underline::DashDotHeavy,
        UnderlineStyle::DotDotDash => Underline::DotDotDash,
        UnderlineStyle::DashDotDotHeavy => Underline::DashDotDotHeavy,
        UnderlineStyle::WavyHeavy => Underline::WavyHeavy,
        UnderlineStyle::WavyDouble => Underline::WavyDouble,
    }
}

/// The highlight colour named `name` (`w:highlight`'s value).
pub(crate) fn highlight_color(name: &str) -> Option<HighlightColor> {
    use HighlightColor as H;
    [
        H::Yellow,
        H::BrightGreen,
        H::Turquoise,
        H::Pink,
        H::Blue,
        H::Red,
        H::DarkBlue,
        H::Teal,
        H::Green,
        H::Violet,
        H::DarkRed,
        H::DarkYellow,
        H::Gray50,
        H::Gray25,
        H::Black,
    ]
    .into_iter()
    .find(|&h| highlight_value(h) == name)
}

/// A highlight colour as authored properties hold it.
pub(crate) fn highlight(c: HighlightColor) -> crate::format::Highlight {
    use crate::format::Highlight as H;
    match c {
        HighlightColor::Yellow => H::Yellow,
        HighlightColor::BrightGreen => H::Green,
        HighlightColor::Turquoise => H::Cyan,
        HighlightColor::Pink => H::Magenta,
        HighlightColor::Blue => H::Blue,
        HighlightColor::Red => H::Red,
        HighlightColor::DarkBlue => H::DarkBlue,
        HighlightColor::Teal => H::DarkCyan,
        HighlightColor::Green => H::DarkGreen,
        HighlightColor::Violet => H::DarkMagenta,
        HighlightColor::DarkRed => H::DarkRed,
        HighlightColor::DarkYellow => H::DarkYellow,
        HighlightColor::Gray50 => H::DarkGray,
        HighlightColor::Gray25 => H::LightGray,
        HighlightColor::Black => H::Black,
    }
}

pub(crate) fn highlight_value(c: HighlightColor) -> &'static str {
    match c {
        HighlightColor::Yellow => "yellow",
        HighlightColor::BrightGreen => "green",
        HighlightColor::Turquoise => "cyan",
        HighlightColor::Pink => "magenta",
        HighlightColor::Blue => "blue",
        HighlightColor::Red => "red",
        HighlightColor::DarkBlue => "darkBlue",
        HighlightColor::Teal => "darkCyan",
        HighlightColor::Green => "darkGreen",
        HighlightColor::Violet => "darkMagenta",
        HighlightColor::DarkRed => "darkRed",
        HighlightColor::DarkYellow => "darkYellow",
        HighlightColor::Gray50 => "darkGray",
        HighlightColor::Gray25 => "lightGray",
        HighlightColor::Black => "black",
    }
}

/// An RGB colour as a colour value.
pub(crate) fn rgb_value(c: AutoOr<Color>) -> AutoOr<ColourValue> {
    match c {
        AutoOr::Auto => AutoOr::Auto,
        AutoOr::Value(c) => AutoOr::Value(ColourValue::rgb(c)),
    }
}

/// An on/off property a command sets.
#[derive(Clone, Copy)]
pub(in crate::editing) enum Flag {
    /// One of the toggles.
    Toggle(crate::format::Toggle),
    /// Double strikethrough.
    DoubleStrike,
}

impl Editor {
    pub(in crate::editing) fn set_toggle(&mut self, t: Toggle, on: bool, fonts: &Fonts) -> Outcome {
        self.format(&Format::Set(t, on), fonts)
    }

    pub(in crate::editing) fn set_flag(&mut self, label: &'static str, flag: Flag, on: bool, fonts: &Fonts) -> Outcome {
        self.format_runs_typed(
            label,
            &|e, _| match flag {
                Flag::Toggle(t) => e.toggle(t, on),
                Flag::DoubleStrike => {
                    e.double_strike = Some(on);
                    e.whole(Prop::DoubleStrike);
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_underline(&mut self, style: UnderlineStyle, fonts: &Fonts) -> Outcome {
        let style = underline(style);
        self.format_runs_typed(
            "Underline",
            &|e, _| {
                e.underline = Some(UnderlineProps { style, color: None });
                e.whole(Prop::Underline);
            },
            fonts,
        )
    }

    /// Sets the text's colour: an RGB colour, a theme colour with a tint or shade, or automatic.
    pub fn set_font_colour(&mut self, c: AutoOr<ColourValue>, fonts: &Fonts) -> Outcome {
        let c = match c {
            AutoOr::Auto => None,
            AutoOr::Value(c) => match self.word_colour(&c) {
                Ok(c) => Some(c),
                Err(why) => return Outcome::invalid(&why),
            },
        };
        self.format(&Format::FontColor(c), fonts)
    }

    pub(in crate::editing) fn set_highlight(&mut self, h: Option<HighlightColor>, fonts: &Fonts) -> Outcome {
        self.format(&Format::Highlight(h.map(highlight)), fonts)
    }

    pub(in crate::editing) fn set_character_spacing(&mut self, s: Points, fonts: &Fonts) -> Outcome {
        let tw = (s.0 * 20.0).round();
        self.format_runs_typed(
            "Character Spacing",
            &|e, _| {
                e.spacing = (tw != 0.0).then_some(tw / 20.0);
                e.whole(Prop::Spacing);
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_character_position(&mut self, s: Points, fonts: &Fonts) -> Outcome {
        let hp = (s.0 * 2.0).round();
        self.format_runs_typed(
            "Position",
            &|e, _| {
                e.position = (hp != 0.0).then_some(hp / 2.0);
                e.whole(Prop::Position);
            },
            fonts,
        )
    }

    pub(in crate::editing) fn apply_character_style(&mut self, id: Option<StyleId>, fonts: &Fonts) -> Outcome {
        use crate::docx::styles::StyleKind;
        let unstyle = |e: &mut CharEdit, _: &RunStyle| {
            e.style = None;
            e.whole(Prop::Style);
        };
        let Some(id) = id else { return self.format_runs_typed("Style", &unstyle, fonts) };
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        // A linked paragraph style's character half.
        let id = match res.styles.by_id.get(&id.0) {
            Some(s) if s.kind == StyleKind::Character => id.0,
            Some(s) if s.kind == StyleKind::Paragraph => {
                match s.link.as_ref().filter(|l| res.styles.by_id.get(*l).is_some_and(|c| c.kind == StyleKind::Character)) {
                    Some(l) => l.clone(),
                    None => return Outcome::invalid("a paragraph style with no linked character style"),
                }
            }
            Some(_) => return Outcome::invalid("not a character style"),
            None => return Outcome::invalid("no such style"),
        };
        self.format(&Format::CharStyle(id), fonts)
    }

    /// Sets the text's typography and, in the same step, its OpenType ligature flags and stylistic sets (each set, or
    /// kept).
    pub fn set_typography_with(&mut self, t: Typography, more: &OpenTypeChange, fonts: &Fonts) -> Outcome {
        if let Some(sets) = &more.stylistic_sets
            && sets.iter().any(|n| !(1..=20).contains(n))
        {
            return Outcome::invalid("a stylistic set is 1…20");
        }
        self.format_runs_typed(
            "Advanced Font",
            &|p, _| {
                if let Some(s) = t.scale {
                    let pct = s.round();
                    p.scale = (pct != 100.0).then_some(pct);
                    p.whole(Prop::Scale);
                }
                if let Some(k) = t.kerning {
                    p.kern = Some((k.0 * 2.0).round() / 2.0);
                    p.whole(Prop::Kern);
                }
                let preset = t.ligatures.map(|l| match l {
                    Ligatures::None => 0,
                    Ligatures::Standard => 1,
                    Ligatures::StandardContextual => 3,
                    Ligatures::HistoricalDiscretional => 12,
                    Ligatures::All => 15,
                });
                if let Some(bits) = more.ligatures.map(LigatureFlags::bits).or(preset) {
                    p.typography.ligatures = Some(bits);
                    p.whole(Prop::Ligatures);
                }
                if let Some(n) = t.number_spacing {
                    use crate::format::NumberSpacing as N;
                    p.whole(Prop::NumberSpacing);
                    p.typography.number_spacing = Some(match n {
                        NumberSpacing::Default => N::Default,
                        NumberSpacing::Proportional => N::Proportional,
                        NumberSpacing::Tabular => N::Tabular,
                    });
                }
                if let Some(n) = t.number_form {
                    use crate::format::NumberForm as N;
                    p.whole(Prop::NumberForm);
                    p.typography.number_form = Some(match n {
                        NumberForm::Default => N::Default,
                        NumberForm::Lining => N::Lining,
                        NumberForm::OldStyle => N::OldStyle,
                    });
                }
                let sets = more.stylistic_sets.clone().or_else(|| t.stylistic_set.map(|n| if n == 0 { Vec::new() } else { vec![n] }));
                if let Some(sets) = sets {
                    // None: no sets, so a style's don't apply
                    p.typography.stylistic_sets = Some(sets.iter().filter(|n| (1..=20).contains(*n)).fold(0, |m, n| m | 1 << (n - 1)));
                    p.whole(Prop::StylisticSets);
                }
                if let Some(on) = t.contextual_alternates {
                    p.typography.contextual_alternates = Some(on);
                    p.whole(Prop::ContextualAlternates);
                }
            },
            fonts,
        )
    }

    /// Sets the text's font to the theme's heading or body font (`+Headings`, `+Body`): it follows the theme.
    pub fn set_theme_font(&mut self, f: ThemeFont, fonts: &Fonts) -> Outcome {
        let face = match f {
            ThemeFont::Major => ThemeFace::Major,
            ThemeFont::Minor => ThemeFace::Minor,
        };
        let theme = Some(FontRef::Theme { face, script: ScriptClass::LatinExtended });
        self.format_runs_typed(
            "Font",
            &|e, _| {
                (e.fonts.latin, e.fonts.latin_extended) = (theme.clone(), theme.clone());
                e.part(Prop::Font(ScriptClass::Latin));
                e.part(Prop::Font(ScriptClass::LatinExtended));
            },
            fonts,
        )
    }

    /// Fits the selected text to `width` (Fit Text), or, `None`, lets it take its own width again. A caret in fitted
    /// text acts on its whole region, which it selects.
    pub fn set_fit_text(&mut self, width: Option<Points>, fonts: &Fonts) -> Outcome {
        if let Some(w) = width
            && !(w.0.is_finite() && w.0 > 0.0 && w.0 <= super::bounds::MAX)
        {
            return Outcome::invalid("the width must be more than 0 and at most 1584");
        }
        if self.selection.is_collapsed() {
            let Some(r) = self.fit_region_at_caret() else { return Outcome::unavailable("nothing selected") };
            self.selection = r;
        }
        let ranges = self.selected_ranges();
        // A region id no fitted text in these paragraphs has, so the region doesn't run into another.
        let mut used: Vec<i64> = Vec::new();
        for &(para, _, _) in &ranges {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            used.extend(p.inlines.iter().filter_map(|i| i.run()?.fit_text.map(|f| f.1)));
        }
        used.sort_unstable();
        used.dedup();
        let id = (1..).find(|n| used.binary_search(n).is_err()).unwrap_or(1);
        let tw = width.map(|w| (w.0 * 20.0).round());
        self.format_runs_typed(
            "Fit Text",
            &|e, _| {
                e.fit_text = tw.map(|tw| (tw / 20.0, id));
                e.whole(Prop::FitText);
            },
            fonts,
        )
    }

    /// The fitted region a collapsed caret is in, as a selection of it.
    fn fit_region_at_caret(&mut self) -> Option<crate::editing::selection::Selection> {
        let caret = self.caret()?;
        let (para, at) = caret.para_offset()?;
        let p = self.index.para(&self.doc, para)?;
        let mut spans: Vec<(u32, u32, Option<i64>)> = Vec::new();
        let mut u = 0u32;
        for i in &p.inlines {
            let n = i.units() as u32;
            if n > 0 {
                let id = i.run().and_then(|r| r.fit_text.map(|f| f.1));
                match spans.last_mut() {
                    Some(last) if last.2 == id && last.1 == u => last.1 = u + n,
                    _ => spans.push((u, u + n, id)),
                }
            }
            u += n;
        }
        let &(a, b, _) = spans.iter().find(|s| s.2.is_some() && s.0 <= at.0 && at.0 <= s.1)?;
        let pos = |x: u32| crate::editing::position::Position::text(caret.story, para, crate::layout::Unit(x));
        Some(crate::editing::selection::Selection::range(pos(a), pos(b), crate::editing::selection::Granularity::Char))
    }

    pub(in crate::editing) fn set_text_effects(&mut self, change: TextEffectsChange, fonts: &Fonts) -> Outcome {
        use crate::api::drawing::EffectEdit;
        use crate::docx::char_props::{effect_off, outline_as_set};
        use crate::docx::model::TextEffects;
        use crate::format::Effect;
        let e = match change.to_model() {
            Ok(e) => e,
            Err(why) => return Outcome::invalid(&why),
        };
        let cx = self.formatted_part_cx();
        let has = |t: Option<&TextEffects>, which: Effect| {
            t.is_some_and(|t| match which {
                Effect::Glow => t.glow.is_some(),
                Effect::Shadow => t.shadow.is_some(),
                Effect::Reflection => t.reflection.is_some(),
                _ => t.outline.is_some(),
            })
        };
        let removed = [
            (Effect::Glow, e.glow.is_remove()),
            (Effect::Shadow, e.shadow.is_remove()),
            (Effect::Reflection, e.reflection.is_remove()),
            (Effect::Outline, e.outline.is_remove()),
        ];
        fn put<T: Clone>(p: &mut CharEdit, which: Effect, edit: &EffectEdit<T>, slot: impl FnOnce(&mut TextEffects) -> &mut Option<T>) {
            match edit {
                EffectEdit::Keep => return,
                EffectEdit::Remove => *slot(std::rc::Rc::make_mut(p.effects.get_or_insert_with(Default::default))) = None,
                EffectEdit::Set(v) => *slot(std::rc::Rc::make_mut(p.effects.get_or_insert_with(Default::default))) = Some(v.clone()),
            }
            p.whole(Prop::Effect(which));
        }
        // None left: no effects, but a fill kept as read that reads as none
        let tidy = |p: &mut CharEdit| {
            let fill_kept = p.foreign.as_ref().is_some_and(|f| f.kept.iter().any(|c| &*c.name.local == "textFill"));
            if p.effects.as_deref().is_some_and(TextEffects::is_empty) && !fill_kept {
                p.effects = None;
            }
        };
        let outline = match &e.outline {
            EffectEdit::Set(l) => EffectEdit::Set(outline_as_set(l)),
            EffectEdit::Remove => EffectEdit::Remove,
            EffectEdit::Keep => EffectEdit::Keep,
        };
        self.format_runs_typed_settled(
            "Text Effects",
            &|p, run| {
                // Shown without an effect of the text's own, an effect is its style's: removing it turns it off.
                let inherited = removed.map(|(k, r)| r && !has(p.effects.as_deref(), k) && has(run.effects.as_deref(), k));
                put(p, Effect::Glow, &e.glow, |t| &mut t.glow);
                put(p, Effect::Shadow, &e.shadow, |t| &mut t.shadow);
                put(p, Effect::Reflection, &e.reflection, |t| &mut t.reflection);
                put(p, Effect::Outline, &outline, |t| &mut t.outline);
                put(p, Effect::Fill, &e.fill, |t| &mut t.fill);
                for ((k, _), on) in removed.iter().zip(inherited) {
                    if on {
                        effect_off(p, *k, &cx);
                    }
                }
                tidy(p);
            },
            // Removed and still shown: the style under the run gives it, and the run turns it off.
            &|p, run| {
                let mut changed = false;
                for (k, _) in removed.iter().filter(|(k, r)| *r && has(run.effects.as_deref(), *k)) {
                    effect_off(p, *k, &cx);
                    changed = true;
                }
                changed
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_language(&mut self, l: Language, fonts: &Fonts) -> Outcome {
        self.format_runs_typed(
            "Language",
            &|p, _| {
                if let Some(tag) = &l.language {
                    p.lang.latin = Some(tag.as_str().into());
                    p.part(Prop::Lang(ScriptClass::Latin));
                }
                if let Some(off) = l.no_proof {
                    p.no_proof = Some(off);
                    p.whole(Prop::NoProof);
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_character_effects(&mut self, e: CharacterEffects, fonts: &Fonts) -> Outcome {
        // Emboss and engrave exclude each other, as in Word.
        let engrave = e.engrave.or(e.emboss.filter(|on| *on).map(|_| false));
        let emboss = e.emboss.or(e.engrave.filter(|on| *on).map(|_| false));
        use crate::format::Toggle as T;
        let flags = [(T::EMBOSS, emboss), (T::IMPRINT, engrave), (T::OUTLINE, e.outline), (T::SHADOW, e.shadow)];
        self.format_runs_typed(
            "Font Effects",
            &|p, _| {
                for (t, on) in flags {
                    if let Some(on) = on {
                        p.toggle(t, on);
                    }
                }
            },
            fonts,
        )
    }

    /// Sets the underline's colour: an RGB colour, a theme colour with a tint or shade, or automatic (the text's).
    pub fn set_underline_colour(&mut self, c: AutoOr<ColourValue>, fonts: &Fonts) -> Outcome {
        let c = match c {
            AutoOr::Auto => None,
            AutoOr::Value(c) => match self.word_colour(&c) {
                Ok(c) => Some(c),
                Err(why) => return Outcome::invalid(&why),
            },
        };
        let color = c.map_or(crate::format::Color::Auto, crate::editing::format::colour);
        self.format_runs_typed(
            "Underline Color",
            &|p, run| {
                // Only underlined text has an underline colour: its underline written with it.
                let Some(u) = run.underline else { return };
                let style = p.underline.map_or(u, |own| own.style);
                p.underline = Some(UnderlineProps { style, color: Some(color) });
                p.part(Prop::UnderlineColor);
            },
            fonts,
        )
    }

    /// What Change Case works on: the selected text, or the word at a collapsed caret.
    fn case_ranges(&mut self) -> Option<Vec<(crate::layout::ParaId, crate::layout::Unit, crate::layout::Unit)>> {
        if !self.selection.is_collapsed() {
            return Some(self.selected_ranges().into_iter().filter(|r| r.1 < r.2).collect());
        }
        let (para, at) = self.caret()?.para_offset()?;
        let (a, b) = crate::editing::text::UnitText::of(self.index.para(&self.doc, para)?).word_touching(at)?;
        Some(vec![(para, a, b)])
    }

    /// The case Shift+F3 changes the selection (the word at a collapsed caret) to next: lowercase text to UPPERCASE,
    /// UPPERCASE to Capitalize Each Word (Sentence case for a selection ending a sentence), any other to lowercase.
    pub fn case_cycle(&mut self) -> Option<CaseChange> {
        let ranges = self.case_ranges()?;
        let (mut upper, mut lower, mut last) = (false, false, None);
        for &(para, a, b) in &ranges {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut at = 0u32;
            let code = field_code_depth(&p.inlines);
            for (k, i) in p.inlines.iter().enumerate() {
                let n = i.units() as u32;
                let (s, e) = (at, at + n);
                at = e;
                let Inline::Text(text, _) = i else { continue };
                // Field codes keep their case, so they don't count toward it.
                if code[k] > 0 || e <= a.0 || s >= b.0 {
                    continue;
                }
                let (from, to) = (a.0.max(s) - s, b.0.min(e) - s);
                for c in text.chars().skip(from as usize).take((to - from) as usize) {
                    upper |= c.is_uppercase();
                    lower |= c.is_lowercase();
                    if !c.is_whitespace() {
                        last = Some(c);
                    }
                }
            }
        }
        let sentence = !self.selection.is_collapsed() && matches!(last, Some('.' | '!' | '?'));
        Some(match (lower, upper) {
            (true, false) => CaseChange::Upper,
            (false, true) if sentence => CaseChange::Sentence,
            (false, true) => CaseChange::Title,
            _ => CaseChange::Lower,
        })
    }

    pub(in crate::editing) fn change_case(&mut self, to: CaseChange, fonts: &Fonts) -> Outcome {
        if self.refuse_expanded() || self.refuse_locked_content() {
            return Outcome::Unchanged("locked");
        }
        let Some(ranges) = self.case_ranges() else { return Outcome::unavailable("no word at the caret") };
        // The word at a caret reaches past what the caret's own checks cover.
        if let (crate::editing::selection::Selection::Caret(c), [(para, a, b)]) = (self.selection, &ranges[..]) {
            let at = |u| crate::editing::position::Position::text(c.story, *para, u);
            if self.refuse_expanded_in(&at(*a), &at(*b)) || self.refuse_locked_content_in(&at(*a), &at(*b)) {
                return Outcome::Unchanged("locked");
            }
        }
        let mut tx = self.begin("Change Case");
        for &(para, a, b) in ranges.iter().rev() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut with = Vec::new();
            let mut at = 0u32;
            let (mut word, mut sentence) = (true, true);
            let code = field_code_depth(&p.inlines);
            for (k, i) in p.inlines.iter().enumerate() {
                let in_code = code[k];
                let n = i.units() as u32;
                let (s, e) = (at, at + n);
                at = e;
                if n == 0 {
                    if s > a.0 && s < b.0 {
                        with.push(i.clone());
                    }
                    continue;
                }
                if e <= a.0 || s >= b.0 {
                    continue;
                }
                match i {
                    Inline::Text(text, run) => {
                        let (from, to_) = (a.0.max(s) - s, b.0.min(e) - s);
                        let part: String = text.chars().skip(from as usize).take((to_ - from) as usize).collect();
                        let part = if in_code > 0 { part } else { recase(&part, to, &mut word, &mut sentence) };
                        with.push(Inline::Text(part, run.clone()));
                    }
                    other => {
                        word = true;
                        with.push(other.clone());
                    }
                }
            }
            if self.step(&mut tx, Step::Text { para, range: a..b, with, edges: Default::default() }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("changing case failed");
            }
        }
        self.keep_selection_through(&tx);
        // A caret inside the word stays as far from its end.
        if let (crate::editing::selection::Selection::Caret(c), [(para, _, b)]) = (self.selection, &ranges[..])
            && let Some((p, at)) = c.para_offset()
            && p == *para
        {
            let end = tx.maps.iter().fold(crate::editing::position::Position::text(c.story, p, *b), |q, m| m.apply(q));
            if let Some((_, e)) = end.para_offset() {
                self.selection = crate::editing::selection::Selection::Caret(crate::editing::position::Position::text(
                    c.story,
                    p,
                    Unit(e.0.saturating_sub(b.0 - at.0)),
                ));
            }
        }
        self.commit(tx, None, fonts)
    }

    fn keep_selection_through(&mut self, tx: &Tx) {
        let map = |p: crate::editing::position::Position| tx.maps.iter().fold(p, |p, m| m.apply(p));
        if let crate::editing::selection::Selection::Text { anchor, focus, unit } = self.selection {
            self.selection = crate::editing::selection::Selection::range(map(anchor), map(focus), unit);
        }
    }

    /// Holds the selection's formatting: its first character's (the caret's, collapsed), and its paragraph's when it
    /// holds a whole paragraph or is collapsed, as Word's format painter does.
    pub(in crate::editing) fn copy_formatting(&mut self) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::unavailable("no styles") };
        let cx = res.styles.cx();
        let ranges = self.selected_ranges();
        let collapsed = self.selection.is_collapsed();
        let (para, run) = match (collapsed, ranges.first()) {
            (true, _) => {
                let Some(caret) = self.caret() else { return Outcome::unavailable("no caret") };
                let Some((para, _)) = caret.para_offset() else { return Outcome::unavailable("no paragraph") };
                (para, self.run_at(&caret))
            }
            (false, Some(&(para, from, _))) => (para, self.first_run(para, from)),
            (false, None) => return Outcome::unavailable("no text selected"),
        };
        let whole = collapsed || !self.selected_marks(&ranges).is_empty();
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::unavailable("no paragraph") };
        // Formatting, not what the text is: its revisions, section and mark
        let strip = |x: Option<&str>, part: &str, drop: &[&str]| {
            let mut props = Props::parse(x, part, &cx);
            props.kids.retain(|(n, _)| !drop.contains(&n.as_str()));
            props
        };
        let own = run.as_ref().and_then(|r| run_xml::rpr(&res, r, &cx));
        let mut rpr = strip(own.as_deref(), "rPr", &["ins", "del", "moveFrom", "moveTo", "rPrChange"]);
        if !whole && rpr.get("rStyle").is_none() {
            paragraph_style_rpr(&res.styles, p.style.style_id.as_deref(), &mut rpr, &cx);
        }
        let ppr = whole.then(|| strip(p.style.direct.0.as_deref(), "pPr", &["sectPr", "rPr", "pPrChange"]).xml());
        self.painter = Some((rpr.xml(), ppr));
        self.painter_claim = Some(None);
        Outcome::Moved
    }

    /// Whether Ctrl+Shift+V, pasting clipboard `text` (and `html`), pastes the painter's formatting instead: it holds
    /// formatting Copy Formatting took and the clipboard hasn't changed since. A copy here changes it; one elsewhere
    /// shows as other clipboard data than the first Ctrl+Shift+V after Copy Formatting saw.
    pub fn paste_takes_formatting(&mut self, text: &str, html: Option<&str>) -> bool {
        let Some(claim) = self.painter_claim.filter(|_| self.painter.is_some()) else { return false };
        let now = clipboard_fingerprint(text, html);
        match claim {
            Some(seen) if seen != now => {
                self.painter_claim = None;
                false
            }
            _ => {
                self.painter_claim = Some(Some(now));
                true
            }
        }
    }

    /// The clipboard as it is at Copy Formatting (`text`, `html`), when the host can tell: Ctrl+Shift+V then pastes
    /// formatting only while the clipboard is still that, so a copy made elsewhere before the first Ctrl+Shift+V counts.
    pub fn clipboard_at_copy_formatting(&mut self, text: &str, html: Option<&str>) {
        if self.painter_claim.is_some() && self.painter.is_some() {
            self.painter_claim = Some(Some(clipboard_fingerprint(text, html)));
        }
    }

    /// The run of `para`'s first unit from `from`, or its mark's.
    fn first_run(&self, para: crate::layout::ParaId, from: crate::layout::Unit) -> Option<crate::docx::Run> {
        let p = self.index.para(&self.doc, para)?;
        let mut at = 0;
        for i in &p.inlines {
            let n = i.units() as u32;
            if n > 0
                && at + n > from.0
                && let Some(r) = i.run()
            {
                return Some(r.clone());
            }
            at += n;
        }
        Some(p.mark.clone())
    }

    /// Gives the selection the held formatting in place of its own: its runs' (the word at a collapsed caret), and its
    /// paragraphs' when the painter took a paragraph's. One undo step.
    pub(in crate::editing) fn paste_formatting(&mut self, fonts: &Fonts) -> Outcome {
        use crate::docx::write::props::{PPR, RPR};
        let Some((rpr, ppr)) = self.painter.clone() else { return Outcome::unavailable("no formatting held") };
        let Some(res) = self.doc.resolver.clone() else { return Outcome::unavailable("no styles") };
        let cx = res.styles.cx();
        let held = Props::parse(rpr.as_deref(), "rPr", &cx);
        let held_ppr = ppr.map(|x| Props::parse(x.as_deref(), "pPr", &cx));
        self.joined("Paste Formatting", fonts, |e| {
            let runs = e.format_runs(
                "Paste Formatting",
                &|p, _, _| {
                    p.kids.retain(|(n, _)| matches!(n.as_str(), "ins" | "del" | "moveFrom" | "moveTo" | "rPrChange"));
                    for (n, x) in &held.kids {
                        p.set(n, x.clone());
                    }
                    p.reorder(RPR);
                },
                fonts,
            );
            let Some(held) = &held_ppr else { return runs };
            if matches!(runs, Outcome::Refused(_)) {
                return runs;
            }
            e.format_paras_direct(
                "Paste Formatting",
                &|p, _, _| {
                    p.kids.retain(|(n, _)| matches!(n.as_str(), "sectPr" | "rPr" | "pPrChange"));
                    for (n, x) in &held.kids {
                        p.set(n, x.clone());
                    }
                    p.reorder(PPR);
                },
                fonts,
            )
        })
    }
}

/// Paragraph style `style`'s character formatting under the direct formatting `rpr`: its linked character style, else
/// the run properties of it and its bases.
fn paragraph_style_rpr(styles: &crate::docx::styles::Styles, style: Option<&str>, rpr: &mut Props, cx: &crate::docx::write::props::Cx) {
    use crate::docx::styles::StyleKind;
    let style = styles.paragraph_style(style);
    let linked = style.and_then(|id| styles.get(id)?.link.as_deref()).filter(|l| styles.get(l).is_some_and(|c| c.kind == StyleKind::Character));
    if let Some(l) = linked {
        let w = &cx.w;
        rpr.set("rStyle", format!("<{w}rStyle {w}val=\"{}\"/>", crate::docx::write::props::esc(l)));
        return;
    }
    let direct: Vec<String> = rpr.kids.iter().map(|(n, _)| n.clone()).collect();
    for s in styles.chain(style) {
        let Some(own) = Props::parse(Some(&s.raw), "style", cx).get("rPr").map(str::to_string) else { continue };
        for (n, x) in Props::parse(Some(&own), "rPr", cx).kids {
            if !direct.contains(&n) && !matches!(n.as_str(), "rStyle" | "rPrChange") {
                rpr.set(&n, x);
            }
        }
    }
}

fn recase(text: &str, to: CaseChange, word: &mut bool, sentence: &mut bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let up = |c: char| c.to_uppercase().collect::<String>();
        let low = |c: char| c.to_lowercase().collect::<String>();
        let s = match to {
            CaseChange::Lower => low(c),
            CaseChange::Upper => up(c),
            CaseChange::Toggle if c.is_uppercase() => low(c),
            CaseChange::Toggle => up(c),
            CaseChange::Title if *word => up(c),
            CaseChange::Title => low(c),
            CaseChange::Sentence if *sentence && c.is_alphabetic() => up(c),
            CaseChange::Sentence => low(c),
        };
        out.push_str(&s);
        if c.is_alphanumeric() {
            *word = false;
            *sentence = false;
        } else {
            *word = true;
            if matches!(c, '.' | '!' | '?') {
                *sentence = true;
            }
        }
    }
    out
}

#[test]
fn cases() {
    let run = |t: &str, to| recase(t, to, &mut true, &mut true);
    assert_eq!(run("hello World. again", CaseChange::Sentence), "Hello world. Again");
    assert_eq!(run("hello wORLD", CaseChange::Title), "Hello World");
    assert_eq!(run("Hello", CaseChange::Toggle), "hELLO");
    assert_eq!(run("Hello", CaseChange::Upper), "HELLO");
    assert_eq!(run("Hello", CaseChange::Lower), "hello");
}

/// What tells one clipboard's content from another's.
fn clipboard_fingerprint(text: &str, html: Option<&str>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (text, html).hash(&mut h);
    h.finish()
}

/// For each of `inlines`, how many fields' instruction text it is in.
fn field_code_depth(inlines: &[Inline]) -> Vec<i32> {
    let mut depth = vec![0i32; inlines.len() + 1];
    for f in crate::docx::fields::spans(inlines) {
        depth[f.begin + 1] += 1;
        depth[f.code_end()] -= 1;
    }
    let mut run = 0;
    for d in &mut depth {
        run += *d;
        *d = run;
    }
    depth
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_attribute_is_found_outside_other_attributes_values() {
        let tag = r#"<w:lang w:eastAsia='x w:val="ja-JP"' w:val = "fr-FR"/>"#;
        assert_eq!(super::with_attr(tag, "w:val", "en-US"), r#"<w:lang w:eastAsia='x w:val="ja-JP"' w:val = "en-US"/>"#);
        assert_eq!(super::with_attr("<w:lang/>", "w:val", "en-US"), r#"<w:lang w:val="en-US"/>"#);
    }

    #[test]
    fn a_single_quoted_value_keeps_its_quotes_closed() {
        let tag = super::with_attr("<w:x w:name='a'/>", "w:name", "O'Brien");
        assert_eq!(tag, "<w:x w:name='O&apos;Brien'/>");
        assert!(roxmltree::Document::parse(&tag.replace("w:", "")).is_ok(), "{tag}");
    }
}
