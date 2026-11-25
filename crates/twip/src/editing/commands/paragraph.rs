//! Paragraph formatting

use crate::api::command::{BorderPreset, Indents, ListKind, Outline, ParagraphFlow, ParagraphSpacing, TextFlow};
use crate::api::value::{Alignment, AutoOr, LineSpacing, StyleId};
use crate::font::Fonts;
use crate::layout::ParaId;

use super::Outcome;
use crate::document::paragraph::{ListId, ListRef};
use crate::editing::Editor;
use crate::editing::format::Format;

fn twips(pt: f64) -> String {
    ((pt * 20.0).round() as i64).to_string()
}

impl Editor {
    pub(in crate::editing) fn set_alignment(&mut self, a: Alignment, fonts: &Fonts) -> Outcome {
        let val = match a {
            Alignment::Left => "left",
            Alignment::Center => "center",
            Alignment::Right => "right",
            Alignment::Justify => "both",
        };
        self.format_paras("Paragraph Formatting", &|p, cx, _| p.set("jc", format!("<{w}jc {w}val=\"{val}\"/>", w = cx.w)), fonts)
    }

    pub(in crate::editing) fn set_line_spacing(&mut self, s: LineSpacing, fonts: &Fonts) -> Outcome {
        let (line, rule) = match s {
            LineSpacing::Multiple { lines } => (((lines * 240.0).round() as i64).to_string(), "auto"),
            LineSpacing::Exact { size } => (twips(size.0), "exact"),
            LineSpacing::AtLeast { size } => (twips(size.0), "atLeast"),
        };
        self.format_paras("Line Spacing", &|p, cx, _| p.attrs("spacing", &[("line", Some(line.clone())), ("lineRule", Some(rule.into()))], cx), fonts)
    }

    pub(in crate::editing) fn set_paragraph_spacing(&mut self, s: ParagraphSpacing, fonts: &Fonts) -> Outcome {
        let mut attrs: Vec<(&str, Option<String>)> = Vec::new();
        for (name, auto, v) in [("before", "beforeAutospacing", s.before), ("after", "afterAutospacing", s.after)] {
            match v {
                Some(AutoOr::Auto) => attrs.extend([(auto, Some("1".to_string()))]),
                Some(AutoOr::Value(pt)) => attrs.extend([(name, Some(twips(pt.0))), (auto, None)]),
                None => {}
            }
        }
        let contextual = s.contextual;
        self.format_paras(
            "Paragraph Spacing",
            &|p, cx, _| {
                if !attrs.is_empty() {
                    p.attrs("spacing", &attrs, cx);
                }
                if let Some(on) = contextual {
                    p.set("contextualSpacing", flag_xml(&cx.w, "contextualSpacing", on));
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_indents(&mut self, i: Indents, fonts: &Fonts) -> Outcome {
        let mut attrs: Vec<(&str, Option<String>)> = Vec::new();
        if let Some(v) = i.left {
            attrs.extend([("left", Some(twips(v.0))), ("start", None)]);
        }
        if let Some(v) = i.right {
            attrs.extend([("right", Some(twips(v.0))), ("end", None)]);
        }
        if let Some(v) = i.first_line {
            attrs.extend([("firstLine", Some(twips(v.0))), ("hanging", None)]);
        }
        if let Some(v) = i.hanging {
            attrs.extend([("hanging", Some(twips(v.0))), ("firstLine", None)]);
        }
        let mirror = i.mirror;
        self.format_paras(
            "Indent",
            &|p, cx, _| {
                if !attrs.is_empty() {
                    p.attrs("ind", &attrs, cx);
                }
                if let Some(on) = mirror {
                    p.set("mirrorIndents", flag_xml(&cx.w, "mirrorIndents", on));
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_outline(&mut self, o: Outline, fonts: &Fonts) -> Outcome {
        if o.level.is_some_and(|l| l > 9) {
            return Outcome::invalid("the outline level is 0…9");
        }
        self.format_paras(
            "Outline Level",
            &|p, cx, _| {
                let w = &cx.w;
                // Body text over a heading style's level is `9`.
                if let Some(l) = o.level {
                    let v = if l == 0 { 9 } else { l - 1 };
                    p.set("outlineLvl", format!("<{w}outlineLvl {w}val=\"{v}\"/>"));
                }
                if let Some(on) = o.collapsed {
                    p.set("w15:collapsed", if on { cx.w15("collapsed", &[]) } else { cx.w15("collapsed", &[("val", "0")]) });
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_paragraph_text_direction(&mut self, d: TextFlow, fonts: &Fonts) -> Outcome {
        let v = text_direction(d);
        self.format_paras("Text Direction", &|p, cx, _| p.set("textDirection", format!("<{w}textDirection {w}val=\"{v}\"/>", w = cx.w)), fonts)
    }

    pub(in crate::editing) fn set_paragraph_style(&mut self, id: StyleId, fonts: &Fonts) -> Outcome {
        match self.doc.resolver.as_ref().and_then(|r| r.styles.by_id.get(&id.0)) {
            None => return Outcome::invalid("no such style"),
            Some(s) if s.kind == crate::docx::styles::StyleKind::Character => {
                return Outcome::invalid("a character style; applyCharacterStyle applies it");
            }
            Some(s) if s.kind != crate::docx::styles::StyleKind::Paragraph => return Outcome::invalid("not a paragraph style"),
            Some(_) => {}
        }
        self.format(&Format::ParaStyle(id.0), fonts)
    }

    pub(crate) fn para_list(&self, para: ParaId) -> Option<bool> {
        let p = self.index.para(&self.doc, para)?;
        let ListRef { list: ListId(id), level: ilvl } = p.style.num.filter(|ListRef { list: ListId(id), level: _ }| *id != 0)?;
        let r = self.doc.resolver.as_ref()?;
        Some(r.numbering.level(id, ilvl, &r.styles)?.fmt == "bullet")
    }

    pub(in crate::editing) fn set_list(&mut self, kind: Option<ListKind>, fonts: &Fonts) -> Outcome {
        let now: Vec<Option<bool>> = self.selected_paras().into_iter().map(|p| self.para_list(p)).collect();
        match kind {
            Some(k) => {
                let bullet = k == ListKind::Bullet;
                if now.iter().all(|l| *l == Some(bullet)) {
                    return Outcome::Unchanged("already so");
                }
                self.toggle_list(bullet, fonts)
            }
            None if now.iter().all(Option::is_none) => Outcome::Unchanged("not in a list"),
            None => match now.iter().flatten().next() {
                Some(&b) if now.iter().all(|l| *l == Some(b)) => self.toggle_list(b, fonts),
                _ => self.joined("Numbering", fonts, |e| {
                    e.toggle_list(true, fonts);
                    e.toggle_list(true, fonts)
                }),
            },
        }
    }

    pub(in crate::editing) fn set_paragraph_flow(&mut self, f: ParagraphFlow, fonts: &Fonts) -> Outcome {
        let flags = [
            ("keepNext", f.keep_with_next),
            ("keepLines", f.keep_lines_together),
            ("pageBreakBefore", f.page_break_before),
            ("widowControl", f.widow_control),
            ("suppressAutoHyphens", f.dont_hyphenate),
            ("suppressLineNumbers", f.suppress_line_numbers),
        ];
        self.format_paras(
            "Paragraph Formatting",
            &|p, cx, _| {
                let w = &cx.w;
                for (name, on) in flags {
                    match on {
                        Some(true) => p.set(name, format!("<{w}{name}/>")),
                        Some(false) => p.set(name, format!("<{w}{name} {w}val=\"0\"/>")),
                        None => {}
                    }
                }
                match f.textbox_tight_wrap.map(|t| t.ooxml().0) {
                    Some("none") => p.set("textboxTightWrap", String::new()),
                    Some(v) => p.set("textboxTightWrap", format!("<{w}textboxTightWrap {w}val=\"{v}\"/>")),
                    None => {}
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_paragraph_borders(&mut self, b: BorderPreset, fonts: &Fonts) -> Outcome {
        let which = match b {
            BorderPreset::Box => "box",
            BorderPreset::Bottom => "bottom",
            BorderPreset::None => "none",
        };
        self.format(&Format::ParaBorders(which.into()), fonts)
    }
}

/// A paragraph flag as written: on, or explicitly off (over a style's).
fn flag_xml(w: &str, name: &str, on: bool) -> String {
    if on { format!("<{w}{name}/>") } else { format!("<{w}{name} {w}val=\"0\"/>") }
}

/// A text direction's `w:textDirection` value.
pub(in crate::editing) fn text_direction(d: TextFlow) -> &'static str {
    match d {
        TextFlow::LrTb => "lrTb",
        TextFlow::TbRl => "tbRl",
        TextFlow::BtLr => "btLr",
        TextFlow::LrTbV => "lrTbV",
        TextFlow::TbRlV => "tbRlV",
        TextFlow::TbLrV => "tbLrV",
    }
}
