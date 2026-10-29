//! Paragraph formatting: setters over the selected paragraphs.

use crate::api::command::{BorderPreset, Indents, ListKind, ParagraphFlow, ParagraphSpacing};
use crate::api::value::{Alignment, AutoOr, LineSpacing, StyleId};
use crate::font::Fonts;
use crate::layout::ParaId;

use super::Outcome;
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
        self.format_paras("Paragraph Spacing", &|p, cx, _| p.attrs("spacing", &attrs, cx), fonts)
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
        self.format_paras("Indent", &|p, cx, _| p.attrs("ind", &attrs, cx), fonts)
    }

    pub(in crate::editing) fn set_paragraph_style(&mut self, id: StyleId, fonts: &Fonts) -> Outcome {
        if !self.doc.resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key(&id.0)) {
            return Outcome::invalid("no such style");
        }
        self.format(&Format::ParaStyle(id.0), fonts)
    }

    /// The list `para` is in: bullets (`true`) or numbering.
    pub(crate) fn para_list(&self, para: ParaId) -> Option<bool> {
        let p = self.index.para(&self.doc, para)?;
        let (id, ilvl) = p.style.num.filter(|(id, _)| *id != 0)?;
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
            // All of one kind come off at once; a mix first joins one kind, then comes off.
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
