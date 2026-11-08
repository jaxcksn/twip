//! Style edits

use std::rc::Rc;

use crate::api::command::Command;
use crate::api::error::Error;
use crate::docx::props::{Tab, TabKind};
use crate::docx::styles::{StyleEditError, StyleKind, Styles};
use crate::docx::write::props::{Cx, Props, STYLE};
use crate::docx::{Paragraph, RunStyle};
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::step::Step;

/// The commands [`Editor::format_style`] runs on a style: character and paragraph formatting.
pub fn formats_a_style(c: &Command) -> bool {
    matches!(
        c,
        Command::SetBold(_)
            | Command::SetItalic(_)
            | Command::SetUnderline(_)
            | Command::SetStrikethrough(_)
            | Command::SetDoubleStrikethrough(_)
            | Command::SetSuperscript(_)
            | Command::SetSubscript(_)
            | Command::SetSmallCaps(_)
            | Command::SetAllCaps(_)
            | Command::SetHidden(_)
            | Command::SetFont(_)
            | Command::SetFontSize(_)
            | Command::GrowFont
            | Command::ShrinkFont
            | Command::SetFontColor(_)
            | Command::SetHighlight(_)
            | Command::SetCharacterSpacing(_)
            | Command::SetCharacterPosition(_)
            | Command::SetAlignment(_)
            | Command::SetLineSpacing(_)
            | Command::SetParagraphSpacing(_)
            | Command::SetIndents(_)
            | Command::IncreaseIndent
            | Command::DecreaseIndent
            | Command::SetParagraphFlow(_)
            | Command::SetParagraphBorders(_)
            | Command::SetParagraphShading(_)
            | Command::SetParagraphBorderLines(_)
            | Command::SetShading(_)
            | Command::SetTextBorder(_)
            | Command::SetFrame(_)
            | Command::RemoveFrame
            | Command::AddTabStop(_)
            | Command::DeleteTabStop(_)
            | Command::ClearTabStops
    )
}

/// `raw`'s `child` properties element (`w:pPr`, `w:rPr`) edited by `edit`.
/// CT_TblStylePr's children in schema order.
const TBL_STYLE_PR: &[&str] = &["pPr", "rPr", "tblPr", "trPr", "tcPr"];

/// A table style's XML with `child` of its part `region` (a `w:tblStylePr`) edited, made when it has none.
pub(super) fn edit_table_part(raw: &str, region: &str, child: &str, cx: &Cx, edit: impl FnOnce(&mut Props)) -> String {
    let w = &cx.w;
    let mut el = Props::parse(Some(raw), "style", cx);
    let at = el.kids.iter().position(|(n, x)| n == "tblStylePr" && x.contains(&format!("type=\"{region}\"")));
    let blank = format!("<{w}tblStylePr {w}type=\"{region}\"><{w}pPr/></{w}tblStylePr>");
    let mut part = Props::parse(Some(at.map_or(blank.as_str(), |i| el.kids[i].1.as_str())), "tblStylePr", cx);
    part.reorder(TBL_STYLE_PR);
    if at.is_none() {
        part.remove("pPr");
    }
    let mut props = Props::parse(part.get(child), child, cx);
    match child {
        "tcPr" => props.reorder(crate::docx::write::props::TCPR),
        "tblPr" => props.reorder(crate::docx::write::props::TBLPR),
        _ => {}
    }
    edit(&mut props);
    match props.xml() {
        Some(x) => part.set(child, x),
        None => part.remove(child),
    }
    let xml = part.xml().unwrap_or_else(|| format!("<{w}tblStylePr {w}type=\"{region}\"/>"));
    match at {
        Some(i) => el.kids[i].1 = xml,
        None => el.add("tblStylePr", xml),
    }
    el.reorder(STYLE);
    el.xml().unwrap_or_else(|| raw.to_string())
}

pub(super) fn edit_child(raw: &str, child: &str, cx: &Cx, edit: impl FnOnce(&mut Props)) -> String {
    let mut el = Props::parse(Some(raw), "style", cx);
    let mut props = Props::parse(el.get(child), child, cx);
    edit(&mut props);
    match props.xml() {
        Some(x) => el.set(child, x),
        None => el.remove(child),
    }
    el.reorder(STYLE);
    el.xml().unwrap_or_else(|| raw.to_string())
}

impl Editor {
    /// Runs formatting commands on a style's XML instead of the selection, and returns the XML after them. Only
    /// character and paragraph formatting is taken; the document doesn't change.
    pub fn format_style(&mut self, raw: &str, commands: Vec<Command>, fonts: &Fonts) -> Result<String, Error> {
        if let Some(c) = commands.iter().find(|c| !formats_a_style(c)) {
            return Err(Error::invalid(format!("{:?} doesn't format a style", c.id())));
        }
        self.style_target = Some(raw.to_string());
        for c in commands {
            if let Outcome::Refused(e) = self.execute(c, fonts) {
                self.style_target = None;
                return Err(e);
            }
        }
        Ok(self.style_target.take().unwrap_or_default())
    }

    /// The target style as text in it resolves: a paragraph in it (or, for a character style, in the default
    /// paragraph style) and a run of that paragraph, from the styles table with the target in it.
    fn style_target_formatting(&self) -> Option<(Paragraph, crate::docx::Run, Cx)> {
        self.style_formatting(self.style_target.as_ref()?)
    }

    /// A style's XML as text in it resolves: a paragraph in it (or, for a character style, in the default paragraph
    /// style) and a run of that paragraph, from the styles table with that XML in it.
    pub(super) fn style_formatting(&self, raw: &str) -> Option<(Paragraph, crate::docx::Run, Cx)> {
        let raw = &raw.to_string();
        let res = self.doc.resolver.as_ref()?;
        let mut next = (**res).clone();
        next.styles.set_xml(raw.clone()).ok()?;
        let style = next.styles.by_id.values().find(|s| s.raw == *raw)?;
        let cx = next.styles.cx();
        let w = &cx.w;
        let id = crate::docx::write::props::esc(&style.id);
        let (ppr, rpr) = match style.kind {
            StyleKind::Character => (None, Some(format!("<{w}rPr><{w}rStyle {w}val=\"{id}\"/></{w}rPr>"))),
            _ => (Some(format!("<{w}pPr><{w}pStyle {w}val=\"{id}\"/></{w}pPr>")), None),
        };
        let para = next.para_from_xml(ppr.as_deref(), None, &cx);
        let run = next.run_from_xml(&para.base, rpr.as_deref(), &cx);
        let p = Paragraph { style: Rc::new(para.style), mark: run.clone(), list: None, inlines: Vec::new(), src: None, id: 0, controls: Vec::new() };
        Some((p, run, cx))
    }

    /// Runs a formatting command's edit on the target style's `part` (`w:pPr`, `w:rPr`): the style's own, or its
    /// table part's (`style_part`).
    pub(super) fn format_style_part(&mut self, part: &str, edit: &dyn Fn(&mut Props, &Cx, &Paragraph, &RunStyle)) -> Outcome {
        let Some((para, run, cx)) = self.style_target_formatting() else { return Outcome::Unchanged("no style") };
        let Some(raw) = self.style_target.take() else { return Outcome::Unchanged("no style") };
        self.style_target = Some(match self.style_part {
            Some(region) => edit_table_part(&raw, region, part, &cx, |p| edit(p, &cx, &para, &run)),
            None => edit_child(&raw, part, &cx, |p| edit(p, &cx, &para, &run)),
        });
        Outcome::Moved
    }

    /// The target style's tab stops, and those it inherits from its base, when commands edit a style.
    pub(super) fn style_tabs(&self) -> Option<(Vec<Tab>, Vec<Tab>)> {
        let (para, _, cx) = self.style_target_formatting()?;
        let res = self.doc.resolver.as_ref()?;
        let base = res
            .styles
            .by_id
            .values()
            .find(|s| Some(s.raw.as_str()) == self.style_target.as_deref())
            .and_then(|s| s.based_on.clone())
            .map(|b| format!("<{w}pPr><{w}pStyle {w}val=\"{}\"/></{w}pPr>", crate::docx::write::props::esc(&b), w = cx.w));
        let inherited = res.para_from_xml(base.as_deref(), None, &cx).style.tabs.iter().filter(|t| t.kind != TabKind::Clear).copied().collect();
        Some((para.style.tabs.clone(), inherited))
    }

    /// The selection's direct formatting, as a style takes it: its paragraph's `w:pPr` and its first run's `w:rPr`,
    /// without what isn't formatting (the style, list, section break, tracked changes).
    pub(super) fn selection_formatting(&self) -> (Option<String>, Option<String>) {
        let Some(res) = self.doc.resolver.as_ref() else { return (None, None) };
        let cx = res.styles.cx();
        let para = self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para(&self.doc, p));
        let strip = |x: &str, part: &str, drop: &[&str]| {
            let mut p = Props::parse(Some(x), part, &cx);
            for k in drop {
                p.remove(k);
            }
            p.xml()
        };
        let ppr = para.and_then(|p| p.style.direct.0.as_deref()).and_then(|x| strip(x, "pPr", &["pStyle", "numPr", "sectPr", "rPr", "pPrChange"]));
        let run = self.caret().and_then(|c| self.run_at(&c));
        let rpr = run.and_then(|r| r.direct.0.as_deref().map(str::to_string)).and_then(|x| strip(&x, "rPr", &["rStyle", "ins", "del", "rPrChange"]));
        (ppr, rpr)
    }

    /// Changes a style to the selection's formatting.
    pub(super) fn update_style_to_selection(&mut self, id: &str, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let Some(style) = res.styles.get(id) else { return Outcome::Refused(Error::unavailable("no such style")) };
        let cx = res.styles.cx();
        let (ppr, rpr) = self.selection_formatting();
        let mut raw = style.raw.clone();
        for (part, own) in [("pPr", ppr), ("rPr", rpr)] {
            let Some(own) = own else { continue };
            if part == "pPr" && style.kind == StyleKind::Character {
                continue;
            }
            let theirs = Props::parse(Some(&own), part, &cx);
            raw = edit_child(&raw, part, &cx, |p| {
                for (name, xml) in &theirs.kids {
                    p.set(name, xml.clone());
                }
            });
        }
        if raw == style.raw {
            return Outcome::Unchanged("the style already matches");
        }
        self.edit_styles("Update Style", |s| s.set_xml(raw), fonts)
    }

    /// Sets the selected paragraphs' border lines.
    pub(super) fn set_paragraph_border_lines(&mut self, b: crate::api::command::ParagraphBorderLines, fonts: &Fonts) -> Outcome {
        let line = |w: &str, side: &str, v: &crate::api::command::Border, shadow: bool| {
            let color = match v.color {
                crate::api::value::AutoOr::Auto => "auto".to_string(),
                crate::api::value::AutoOr::Value(c) => format!("{:06X}", c.0),
            };
            let shadow = if shadow { format!(" {w}shadow=\"1\"") } else { String::new() };
            format!(
                "<{w}{side} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"1\" {w}color=\"{color}\"{shadow}/>",
                v.style.val(),
                (v.width.0 * 8.0).round() as i64
            )
        };
        let shadow = b.shadow.unwrap_or(false);
        let edges = [
            ("top", b.top.clone()),
            ("left", b.left.clone()),
            ("bottom", b.bottom.clone()),
            ("right", b.right.clone()),
            ("between", b.between.clone()),
        ];
        self.format_paras(
            "Borders",
            &|p, cx, _| {
                let w = &cx.w;
                let mut bdr = Props::parse(p.get("pBdr"), "pBdr", cx);
                bdr.reorder(&["top", "left", "bottom", "right", "between", "bar"]);
                for (side, v) in &edges {
                    match v {
                        Some(Some(v)) => bdr.set(side, line(w, side, v, shadow && matches!(*side, "bottom" | "right"))),
                        Some(None) => bdr.remove(side),
                        None => {}
                    }
                }
                match bdr.xml() {
                    Some(x) => p.set("pBdr", x),
                    None => p.remove("pBdr"),
                }
            },
            fonts,
        )
    }

    /// Sets or removes the selected text's border (`w:bdr`).
    pub(super) fn set_text_border(&mut self, b: crate::api::command::TextBorderChange, fonts: &Fonts) -> Outcome {
        self.format_runs(
            "Borders",
            &|p, w, _| match &b.border {
                Some(line) => {
                    let color = match line.color {
                        crate::api::value::AutoOr::Auto => "auto".to_string(),
                        crate::api::value::AutoOr::Value(c) => format!("{:06X}", c.0),
                    };
                    let shadow = if b.shadow { format!(" {w}shadow=\"1\"") } else { String::new() };
                    let sz = (line.width.0 * 8.0).round() as i64;
                    p.set("bdr", format!("<{w}bdr {w}val=\"{}\" {w}sz=\"{sz}\" {w}space=\"0\" {w}color=\"{color}\"{shadow}/>", line.style.val()));
                }
                None => p.remove("bdr"),
            },
            fonts,
        )
    }

    /// Sets the shading of the selected paragraphs or text.
    pub(super) fn set_shading(&mut self, s: crate::api::command::ShadingChange, fonts: &Fonts) -> Outcome {
        use crate::api::command::ShadingTarget;
        use crate::api::value::{AutoOr, NoneOr};
        let xml = |w: &str| match s.fill {
            NoneOr::None if s.pattern == crate::api::command::ShadingPattern::Clear => None,
            fill => {
                let fill = match fill {
                    NoneOr::Value(c) => format!("{:06X}", c.0),
                    NoneOr::None => "auto".to_string(),
                };
                let color = match s.color {
                    AutoOr::Auto => "auto".to_string(),
                    AutoOr::Value(c) => format!("{:06X}", c.0),
                };
                Some(format!("<{w}shd {w}val=\"{}\" {w}color=\"{color}\" {w}fill=\"{fill}\"/>", s.pattern.val()))
            }
        };
        match s.target {
            ShadingTarget::Paragraph => self.format_paras(
                "Shading",
                &|p, cx, _| match xml(&cx.w) {
                    Some(x) => p.set("shd", x),
                    None => p.remove("shd"),
                },
                fonts,
            ),
            ShadingTarget::Text => self.format_runs(
                "Shading",
                &|p, w, _| match xml(w) {
                    Some(x) => p.set("shd", x),
                    None => p.remove("shd"),
                },
                fonts,
            ),
            // The selected cells: their fill (cell shading has no pattern here).
            ShadingTarget::Cell => {
                let fill = match s.fill {
                    NoneOr::Value(c) => Some(c),
                    NoneOr::None => None,
                };
                self.execute(Command::SetCellShading(fill), fonts)
            }
        }
    }

    /// Deletes a style the document made.
    pub(super) fn delete_style(&mut self, id: &str, fonts: &Fonts) -> Outcome {
        let Some(style) = self.doc.resolver.as_ref().and_then(|r| r.styles.get(id)).cloned() else {
            return Outcome::Refused(Error::unavailable("no such style"));
        };
        if !style.custom || self.doc.resolver.as_ref().is_some_and(|r| r.styles.default_paragraph.as_deref() == Some(id)) {
            return Outcome::Refused(Error::new(crate::api::error::ErrorCode::NotAllowed, "built-in styles can't be deleted"));
        }
        // shortcut: paragraphs and runs keep naming the deleted style in their XML, so they resolve as the default
        // paragraph style or no character style; a style created later under the same id would apply to them.
        self.edit_styles("Delete Style", |s| s.delete(id), fonts)
    }

    /// Sets a style's list flags.
    pub(super) fn set_style_meta(&mut self, m: crate::api::command::StyleMetaChange, fonts: &Fonts) -> Outcome {
        use crate::api::services::StyleVisibility;
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let Some(style) = res.styles.get(&m.id.0) else { return Outcome::Refused(Error::unavailable("no such style")) };
        let cx = res.styles.cx();
        let w = cx.w.clone();
        let mut el = Props::parse(Some(&style.raw), "style", &cx);
        let flag = |el: &mut Props, name: &str, on: bool| if on { el.set(name, format!("<{w}{name}/>")) } else { el.remove(name) };
        if let Some(v) = m.visibility {
            let (semi, unhide) = match v {
                StyleVisibility::Show => (false, false),
                StyleVisibility::HideUntilUsed => (true, true),
                StyleVisibility::Hide => (true, false),
            };
            flag(&mut el, "semiHidden", semi);
            flag(&mut el, "unhideWhenUsed", unhide);
        }
        if let Some(p) = m.priority {
            el.set("uiPriority", format!("<{w}uiPriority {w}val=\"{p}\"/>"));
        }
        if let Some(q) = m.quick {
            flag(&mut el, "qFormat", q);
        }
        if let Some(l) = m.locked {
            flag(&mut el, "locked", l);
        }
        el.reorder(STYLE);
        let Some(raw) = el.xml() else { return Outcome::Unchanged("no style") };
        if raw == style.raw {
            return Outcome::Unchanged("already so");
        }
        self.edit_styles("Style Options", |s| s.set_xml(raw), fonts)
    }

    /// Sets the document's default formatting: the formatting commands run on the defaults as if they were a style.
    pub(super) fn set_document_defaults(&mut self, d: crate::api::command::DocumentDefaults, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = res.styles.cx();
        let w = cx.w.clone();
        let defaults = res.styles.defaults_raw.clone().unwrap_or_else(|| format!("<{w}docDefaults/>"));
        let parts = Props::parse(Some(&defaults), "docDefaults", &cx);
        let inner = |wrapper: &str, part: &str| {
            parts.get(wrapper).and_then(|x| Props::parse(Some(x), wrapper, &cx).get(part).map(str::to_string)).unwrap_or_default()
        };
        let stand_in = format!(
            "<{w}style {w}type=\"paragraph\" {w}styleId=\"DocumentDefaults\"><{w}name {w}val=\"Document Defaults\"/>{}{}</{w}style>",
            inner("pPrDefault", "pPr"),
            inner("rPrDefault", "rPr")
        );
        let mut commands = Vec::new();
        if let Some(f) = d.font.filter(|f| !f.trim().is_empty()) {
            commands.push(Command::SetFont(f));
        }
        if let Some(s) = d.size {
            commands.push(Command::SetFontSize(s));
        }
        if let Some(c) = d.color {
            commands.push(Command::SetFontColor(c));
        }
        if let Some(s) = d.spacing {
            commands.push(Command::SetParagraphSpacing(s));
        }
        if let Some(l) = d.line_spacing {
            commands.push(Command::SetLineSpacing(l));
        }
        if let Some(i) = d.indents {
            commands.push(Command::SetIndents(i));
        }
        let edited = match self.format_style(&stand_in, commands, fonts) {
            Ok(x) => x,
            Err(e) => return Outcome::Refused(e),
        };
        let el = Props::parse(Some(&edited), "style", &cx);
        let wrap = |name: &str, part: &str| el.get(part).map_or(String::new(), |x| format!("<{w}{name}>{x}</{w}{name}>"));
        let raw = format!("<{w}docDefaults>{}{}</{w}docDefaults>", wrap("rPrDefault", "rPr"), wrap("pPrDefault", "pPr"));
        if Some(&raw) == res.styles.defaults_raw.as_ref() {
            return Outcome::Unchanged("already so");
        }
        self.edit_styles("Set Defaults", |s| s.set_defaults_xml(raw), fonts)
    }

    /// The paragraph style every selected paragraph is in, when it updates automatically (`w:autoRedefine`).
    pub(super) fn auto_redefined(&mut self) -> Option<String> {
        let res = self.doc.resolver.clone()?;
        let paras = self.selected_paras();
        let mut ids = paras
            .iter()
            .map(|&p| self.index.para(&self.doc, p).and_then(|q| q.style.style_id.clone()).or_else(|| res.styles.default_paragraph.clone()));
        let first = ids.next()??;
        if !ids.all(|i| i.as_deref() == Some(first.as_str())) {
            return None;
        }
        res.styles.get(&first).filter(|s| s.meta.auto_redefine).map(|s| s.id.clone())
    }

    /// Paragraph formatting on paragraphs in an automatically updating style: the style changes instead, as one undo
    /// step.
    pub(super) fn format_auto_redefined(&mut self, id: &str, edit: &dyn Fn(&mut Props, &Cx, &Paragraph), fonts: &Fonts) -> Outcome {
        let Some(raw) = self.doc.resolver.as_ref().and_then(|r| r.styles.get(id)).map(|s| s.raw.clone()) else {
            return Outcome::Unchanged("no style");
        };
        self.style_target = Some(raw);
        let _ = self.format_style_part("pPr", &|p, cx, para, _| edit(p, cx, para));
        let Some(raw) = self.style_target.take() else { return Outcome::Unchanged("no style") };
        self.edit_styles("Modify Style", |s| s.set_xml(raw), fonts)
    }

    /// Runs `edit` on the styles table as one undo step named `label`. Every paragraph, run and table whose style
    /// changed (the edit's answer) is formatted again from the new table.
    pub(crate) fn edit_styles(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut Styles) -> Result<Vec<String>, StyleEditError>,
        fonts: &Fonts,
    ) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let mut next = (*res).clone();
        let changed = match edit(&mut next.styles) {
            Ok(c) => c,
            Err(StyleEditError::Missing) => return Outcome::Refused(Error::unavailable("no such style")),
            Err(StyleEditError::Loop) => return Outcome::Refused(Error::invalid("a style can't be based on itself")),
            Err(StyleEditError::Malformed) => return Outcome::Refused(Error::invalid("not a style")),
        };
        let mut tx = self.begin(label);
        let added = self.doc.added_lists.clone();
        if self.step(&mut tx, Step::Resolver { resolver: Some(Rc::new(next)), added }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the styles didn't change");
        }
        let default = self.doc.resolver.as_ref().and_then(|r| r.styles.default_paragraph.clone());
        let uses = |id: Option<&str>| match id.or(default.as_deref()) {
            Some(id) => changed.iter().any(|c| c == id),
            None => false,
        };
        let paras: Vec<_> = self
            .index
            .para_ids()
            .filter(|&p| {
                self.index.para(&self.doc, p).is_some_and(|q| {
                    uses(q.style.style_id.as_deref())
                        || q.inlines.iter().filter_map(|i| i.run()).any(|r| r.style_id.as_deref().is_some_and(|s| changed.iter().any(|c| c == s)))
                })
            })
            .collect();
        let tables: Vec<_> = self
            .index
            .table_ids()
            .filter(|&t| self.index.table(&self.doc, t).and_then(|t| t.style_id.as_deref()).is_some_and(|s| changed.iter().any(|c| c == s)))
            .collect();
        tx.stale_paras.extend(paras);
        tx.stale_tables.extend(tables);
        self.commit(tx, None, fonts)
    }
}
