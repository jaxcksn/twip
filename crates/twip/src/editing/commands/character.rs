//! Character formatting

use crate::api::command::{CaseChange, CharacterEffects, Language, Ligatures, NumberForm, NumberSpacing, Typography};
use crate::api::drawing::TextEffectsChange;
use crate::api::value::{AutoOr, Color, HighlightColor, Points, StyleId, UnderlineStyle};
use crate::docx::Inline;
use crate::docx::RunStyle;
use crate::docx::write::props::Props;
use crate::font::Fonts;

use super::Outcome;
use crate::editing::format::{Format, Toggle};
use crate::editing::step::Step;
use crate::editing::{Editor, Tx};

/// A run toggle as written: on, or explicitly off (over a style's).
fn toggle_xml(w: &str, name: &str, on: bool) -> String {
    if on { format!("<{w}{name}/>") } else { format!("<{w}{name} {w}val=\"0\"/>") }
}

/// `tag` without the attribute `name`.
fn strip(tag: &str, name: &str) -> String {
    let Some(k) = tag.find(&format!(" {name}=\"")) else { return tag.to_string() };
    let from = k + name.len() + 3;
    let to = tag[from..].find('"').map_or(tag.len(), |e| from + e + 1);
    format!("{}{}", &tag[..k], &tag[to..])
}

/// `tag` (one start tag or empty element) with the attribute `name` set.
fn with_attr(tag: &str, name: &str, value: &str) -> String {
    let v = crate::docx::write::props::esc(value);
    if let Some(k) = tag.find(&format!(" {name}=\"")) {
        let from = k + name.len() + 3;
        let to = tag[from..].find('"').map_or(tag.len(), |e| from + e);
        return format!("{}{v}{}", &tag[..from], &tag[to..]);
    }
    let at = tag.rfind("/>").or_else(|| tag.rfind('>')).unwrap_or(tag.len());
    format!("{} {name}=\"{v}\"{}", &tag[..at], &tag[at..])
}

pub(crate) fn underline_value(style: UnderlineStyle) -> &'static str {
    match style {
        UnderlineStyle::None => "none",
        UnderlineStyle::Single => "single",
        UnderlineStyle::Words => "words",
        UnderlineStyle::Double => "double",
        UnderlineStyle::Thick => "thick",
        UnderlineStyle::Dotted => "dotted",
        UnderlineStyle::Dash => "dash",
        UnderlineStyle::DotDash => "dotDash",
        UnderlineStyle::Wave => "wave",
        UnderlineStyle::DottedHeavy => "dottedHeavy",
        UnderlineStyle::DashedHeavy => "dashedHeavy",
        UnderlineStyle::DashLong => "dashLong",
        UnderlineStyle::DashLongHeavy => "dashLongHeavy",
        UnderlineStyle::DashDotHeavy => "dashDotHeavy",
        UnderlineStyle::DotDotDash => "dotDotDash",
        UnderlineStyle::DashDotDotHeavy => "dashDotDotHeavy",
        UnderlineStyle::WavyHeavy => "wavyHeavy",
        UnderlineStyle::WavyDouble => "wavyDouble",
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

impl Editor {
    pub(in crate::editing) fn set_toggle(&mut self, t: Toggle, on: bool, fonts: &Fonts) -> Outcome {
        self.format(&Format::Set(t, on), fonts)
    }

    pub(in crate::editing) fn set_flag(&mut self, label: &'static str, name: &'static str, on: bool, fonts: &Fonts) -> Outcome {
        self.format_runs(label, &|p, w, _| p.set(name, if on { format!("<{w}{name}/>") } else { format!("<{w}{name} {w}val=\"0\"/>") }), fonts)
    }

    pub(in crate::editing) fn set_underline(&mut self, style: UnderlineStyle, fonts: &Fonts) -> Outcome {
        let v = underline_value(style);
        self.format_runs("Underline", &|p, w, _| p.set("u", format!("<{w}u {w}val=\"{v}\"/>")), fonts)
    }

    pub(in crate::editing) fn set_font_color(&mut self, c: AutoOr<Color>, fonts: &Fonts) -> Outcome {
        let c = match c {
            AutoOr::Auto => None,
            AutoOr::Value(c) => Some(c.0),
        };
        self.format(&Format::FontColor(c), fonts)
    }

    pub(in crate::editing) fn set_highlight(&mut self, h: Option<HighlightColor>, fonts: &Fonts) -> Outcome {
        self.format(&Format::Highlight(h.map(|h| highlight_value(h).to_string())), fonts)
    }

    pub(in crate::editing) fn set_character_spacing(&mut self, s: Points, fonts: &Fonts) -> Outcome {
        let tw = (s.0 * 20.0).round() as i64;
        self.format_runs(
            "Character Spacing",
            &|p, w, _| if tw == 0 { p.remove("spacing") } else { p.set("spacing", format!("<{w}spacing {w}val=\"{tw}\"/>")) },
            fonts,
        )
    }

    pub(in crate::editing) fn set_character_position(&mut self, s: Points, fonts: &Fonts) -> Outcome {
        let hp = (s.0 * 2.0).round() as i64;
        self.format_runs(
            "Position",
            &|p, w, _| if hp == 0 { p.remove("position") } else { p.set("position", format!("<{w}position {w}val=\"{hp}\"/>")) },
            fonts,
        )
    }

    pub(in crate::editing) fn apply_character_style(&mut self, id: Option<StyleId>, fonts: &Fonts) -> Outcome {
        use crate::docx::styles::StyleKind;
        let Some(id) = id else { return self.format_runs("Style", &|p, _, _| p.remove("rStyle"), fonts) };
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

    pub(in crate::editing) fn set_typography(&mut self, t: Typography, fonts: &Fonts) -> Outcome {
        // The part the w14 elements go in declares their namespace or not: the styles part when a style is formatted.
        let cx = match (&self.style_target, &self.doc.resolver) {
            (Some(_), Some(r)) => r.styles.cx(),
            _ => self.cx(),
        };
        self.format_runs(
            "Advanced Font",
            &|p, w, _| {
                if let Some(s) = t.scale {
                    let pct = s.round() as i64;
                    if pct == 100 { p.remove("w") } else { p.set("w", format!("<{w}w {w}val=\"{pct}\"/>")) }
                }
                if let Some(k) = t.kerning {
                    p.set("kern", format!("<{w}kern {w}val=\"{}\"/>", (k.0 * 2.0).round() as i64));
                }
                if let Some(l) = t.ligatures {
                    let v = match l {
                        Ligatures::None => "none",
                        Ligatures::Standard => "standard",
                        Ligatures::StandardContextual => "standardContextual",
                        Ligatures::HistoricalDiscretional => "historicalDiscretional",
                        Ligatures::All => "all",
                    };
                    p.set("w14:ligatures", cx.w14("ligatures", &[("val", v)], ""));
                }
                if let Some(n) = t.number_spacing {
                    let v = match n {
                        NumberSpacing::Default => "default",
                        NumberSpacing::Proportional => "proportional",
                        NumberSpacing::Tabular => "tabular",
                    };
                    p.set("w14:numSpacing", cx.w14("numSpacing", &[("val", v)], ""));
                }
                if let Some(n) = t.number_form {
                    let v = match n {
                        NumberForm::Default => "default",
                        NumberForm::Lining => "lining",
                        NumberForm::OldStyle => "oldStyle",
                    };
                    p.set("w14:numForm", cx.w14("numForm", &[("val", v)], ""));
                }
                match t.stylistic_set {
                    Some(0) => p.remove("w14:stylisticSets"),
                    Some(n) => p.set("w14:stylisticSets", cx.w14("stylisticSets", &[], &format!("<{{w14}}styleSet {{w14}}id=\"{n}\"/>"))),
                    None => {}
                }
                if let Some(on) = t.contextual_alternates {
                    p.set("w14:cntxtAlts", cx.w14("cntxtAlts", &[("val", if on { "1" } else { "0" })], ""));
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_text_effects(&mut self, change: TextEffectsChange, fonts: &Fonts) -> Outcome {
        use crate::api::drawing::EffectEdit;
        use crate::docx::write::drawing as xml;
        let e = match change.to_model() {
            Ok(e) => e,
            Err(why) => return Outcome::invalid(&why),
        };
        // The part the w14 elements go in declares their namespace or not: the styles part when a style is formatted.
        let cx = match (&self.style_target, &self.doc.resolver) {
            (Some(_), Some(r)) => r.styles.cx(),
            _ => self.cx(),
        };
        // Turned off over a style's: Word's own forms of no effect.
        let off = |key: &str| -> String {
            let zeros = |names: &[&'static str]| names.iter().map(|&n| (n, "0")).chain([("algn", "none")]).collect::<Vec<_>>();
            let black = "<{w14}srgbClr {w14}val=\"000000\"/>";
            match key {
                "w14:glow" => cx.w14("glow", &[("rad", "0")], black),
                "w14:shadow" => cx.w14("shadow", &zeros(&["blurRad", "dist", "dir", "sx", "sy", "kx", "ky"]), black),
                "w14:reflection" => {
                    cx.w14("reflection", &zeros(&["blurRad", "stA", "stPos", "endA", "endPos", "dist", "dir", "fadeDir", "sx", "sy", "kx", "ky"]), "")
                }
                _ => cx.w14("textOutline", &[], "<{w14}noFill/>"),
            }
        };
        let shows = |r: &RunStyle, key: &str| {
            r.effects.as_deref().is_some_and(|e| match key {
                "w14:glow" => e.glow.is_some(),
                "w14:shadow" => e.shadow.is_some(),
                "w14:reflection" => e.reflection.is_some(),
                _ => e.outline.is_some(),
            })
        };
        let removed = [
            ("w14:glow", e.glow.is_remove()),
            ("w14:shadow", e.shadow.is_remove()),
            ("w14:reflection", e.reflection.is_remove()),
            ("w14:textOutline", e.outline.is_remove()),
        ];
        fn put<T>(p: &mut Props, key: &str, edit: &EffectEdit<T>, xml: impl FnOnce(&T) -> String) {
            match edit {
                EffectEdit::Keep => {}
                EffectEdit::Remove => p.remove(key),
                EffectEdit::Set(v) => p.set(key, xml(v)),
            }
        }
        self.format_runs_settled(
            "Text Effects",
            &|p, _, run| {
                // Shown without an element of the text's own, an effect is its style's: removing it turns it off.
                let inherited = removed.map(|(k, r)| r && p.get(k).is_none() && shows(run, k));
                put(p, "w14:glow", &e.glow, |g| xml::text_glow_xml(&cx, g));
                put(p, "w14:shadow", &e.shadow, |s| xml::text_shadow_xml(&cx, s));
                put(p, "w14:reflection", &e.reflection, |r| xml::text_reflection_xml(&cx, r));
                put(p, "w14:textOutline", &e.outline, |l| xml::text_outline_xml(&cx, l));
                put(p, "w14:textFill", &e.fill, |f| xml::text_fill_xml(&cx, f));
                for ((k, _), on) in removed.iter().zip(inherited) {
                    if on {
                        p.set(k, off(k));
                    }
                }
            },
            // Removed and still shown: the style under the run gives it, and the run turns it off.
            &|p, _, run| {
                let mut changed = false;
                for (key, _) in removed.iter().filter(|(k, r)| *r && shows(run, k)) {
                    p.set(key, off(key));
                    changed = true;
                }
                changed
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_language(&mut self, l: Language, fonts: &Fonts) -> Outcome {
        self.format_runs(
            "Language",
            &|p, w, _| {
                if let Some(tag) = &l.language {
                    let now = p.get("lang").map_or_else(|| format!("<{w}lang/>"), str::to_string);
                    p.set("lang", with_attr(&now, &format!("{w}val"), tag));
                }
                if let Some(off) = l.no_proof {
                    p.set("noProof", toggle_xml(w, "noProof", off));
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_character_effects(&mut self, e: CharacterEffects, fonts: &Fonts) -> Outcome {
        // Emboss and engrave exclude each other, as in Word.
        let engrave = e.engrave.or(e.emboss.filter(|on| *on).map(|_| false));
        let emboss = e.emboss.or(e.engrave.filter(|on| *on).map(|_| false));
        let flags = [("emboss", emboss), ("imprint", engrave), ("outline", e.outline), ("shadow", e.shadow)];
        self.format_runs(
            "Font Effects",
            &|p, w, _| {
                for (name, on) in flags {
                    if let Some(on) = on {
                        p.set(name, toggle_xml(w, name, on));
                    }
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_underline_color(&mut self, c: AutoOr<Color>, fonts: &Fonts) -> Outcome {
        self.format_runs(
            "Underline Color",
            &|p, w, run| {
                // Only underlined text has an underline colour: its underline written with it.
                let Some(u) = run.underline.as_deref() else { return };
                let now = p.get("u").map_or_else(|| format!("<{w}u {w}val=\"{u}\"/>"), str::to_string);
                let now = ["themeColor", "themeTint", "themeShade"].iter().fold(now, |t, a| strip(&t, &format!("{w}{a}")));
                let hex = match c {
                    AutoOr::Auto => "auto".to_string(),
                    AutoOr::Value(c) => format!("{:06X}", c.0),
                };
                p.set("u", with_attr(&now, &format!("{w}color"), &hex));
            },
            fonts,
        )
    }

    pub(in crate::editing) fn change_case(&mut self, to: CaseChange, fonts: &Fonts) -> Outcome {
        if self.selection.is_collapsed() {
            return Outcome::unavailable("nothing selected");
        }
        if self.refuse_expanded() || self.refuse_locked_content() {
            return Outcome::Unchanged("locked");
        }
        let ranges: Vec<_> = self.selected_ranges().into_iter().filter(|r| r.1 < r.2).collect();
        let mut tx = self.begin("Change Case");
        for (para, a, b) in ranges {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut with = Vec::new();
            let mut at = 0u32;
            let (mut word, mut sentence) = (true, true);
            for i in &p.inlines {
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
                        with.push(Inline::Text(recase(&part, to, &mut word, &mut sentence), run.clone()));
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
        self.commit(tx, None, fonts)
    }

    fn keep_selection_through(&mut self, tx: &Tx) {
        let map = |p: crate::editing::position::Position| tx.maps.iter().fold(p, |p, m| m.apply(p));
        if let crate::editing::selection::Selection::Text { anchor, focus, unit } = self.selection {
            self.selection = crate::editing::selection::Selection::range(map(anchor), map(focus), unit);
        }
    }

    pub(in crate::editing) fn copy_formatting(&mut self) -> Outcome {
        Outcome::unsupported("the format painter")
    }

    pub(in crate::editing) fn paste_formatting(&mut self, _fonts: &Fonts) -> Outcome {
        Outcome::unsupported("the format painter")
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
