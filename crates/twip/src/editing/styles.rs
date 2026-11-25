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
    formats_a_style_id(c.id())
}

/// Whether [`formats_a_style`] takes the commands `id` names.
pub fn formats_a_style_id(id: crate::api::command::CommandId) -> bool {
    use crate::api::command::CommandId as C;
    matches!(
        id,
        C::SetBold
            | C::SetItalic
            | C::SetUnderline
            | C::SetStrikethrough
            | C::SetDoubleStrikethrough
            | C::SetSuperscript
            | C::SetSubscript
            | C::SetSmallCaps
            | C::SetAllCaps
            | C::SetHidden
            | C::SetFont
            | C::SetFontSize
            | C::GrowFont
            | C::ShrinkFont
            | C::SetFontColor
            | C::SetHighlight
            | C::SetCharacterSpacing
            | C::SetCharacterPosition
            | C::SetTypography
            | C::SetLanguage
            | C::SetCharacterEffects
            | C::SetTextEffects
            | C::SetUnderlineColor
            | C::SetAlignment
            | C::SetLineSpacing
            | C::SetParagraphSpacing
            | C::SetIndents
            | C::IncreaseIndent
            | C::DecreaseIndent
            | C::SetParagraphFlow
            | C::SetOutline
            | C::SetParagraphTextDirection
            | C::SetParagraphBorders
            | C::SetParagraphShading
            | C::SetParagraphBorderLines
            | C::SetShading
            | C::SetTextBorder
            | C::SetFrame
            | C::RemoveFrame
            | C::AddTabStop
            | C::DeleteTabStop
            | C::ClearTabStops
    )
}

/// `raw`'s `child` properties element (`w:pPr`, `w:rPr`) edited by `edit`.
/// CT_TblStylePr's children in schema order.
const TBL_STYLE_PR: &[&str] = &["pPr", "rPr", "tblPr", "trPr", "tcPr"];

/// A table style's XML with `child` of its part `region` (a `w:tblStylePr`) edited, made when it has none.
pub(super) fn edit_table_part(raw: &str, region: &str, child: &str, cx: &Cx, edit: impl FnOnce(&mut Props)) -> String {
    let w = &cx.w;
    let mut el = Props::parse(Some(raw), "style", cx);
    let at =
        el.kids.iter().position(|(n, x)| n == "tblStylePr" && cx.parse(x, |_, p| crate::docx::xml::attr(p, "type") == Some(region)) == Some(true));
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
        // The defaults formatted as a style: alone, not over themselves.
        if self.defaults_target {
            (next.styles.doc_rpr, next.styles.doc_ppr) = Default::default();
        }
        next.styles.set_xml(raw.clone()).ok()?;
        let style = next.styles.by_id.values().find(|s| s.raw == *raw)?;
        let cx = next.styles.cx();
        let w = &cx.w;
        let id = crate::docx::write::props::esc(&style.id);
        let (ppr, rpr) = match style.kind {
            StyleKind::Character => (None, Some(format!("<{w}rPr><{w}rStyle {w}val=\"{id}\"/></{w}rPr>"))),
            StyleKind::Table => (None, None),
            _ => (Some(format!("<{w}pPr><{w}pStyle {w}val=\"{id}\"/></{w}pPr>")), None),
        };
        // A table style's text: in the part being formatted, else the whole table.
        let region = (style.kind == StyleKind::Table).then(|| next.styles.table_region(Some(&style.id), self.style_part.as_slice()));
        let para = next.para_from_xml(ppr.as_deref(), region.as_ref(), &cx);
        let run = next.run_from_xml(&para.base, rpr.as_deref(), &cx);
        let p = Paragraph {
            style: Rc::new(para.style),
            mark: run.clone(),
            list: None,
            inlines: Vec::new(),
            src: None,
            id: 0,
            div: None,
            controls: Vec::new(),
        };
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

    /// The target style's tab stops, and those it inherits from its base, when commands edit a style: the one
    /// formatting edits, or the one the selected paragraphs are in when it updates automatically.
    pub(super) fn style_tabs(&mut self) -> Option<(Vec<Tab>, Vec<Tab>)> {
        let raw = match &self.style_target {
            Some(raw) => raw.clone(),
            None => {
                let id = self.auto_redefined()?;
                self.doc.resolver.as_ref()?.styles.get(&id)?.raw.clone()
            }
        };
        let (para, _, cx) = self.style_formatting(&raw)?;
        let res = self.doc.resolver.as_ref()?;
        let base = res
            .styles
            .by_id
            .values()
            .find(|s| s.raw == raw)
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

    /// The built-in style `cmd` applies (or updates) when the document doesn't define it, and it is of the kind `cmd`
    /// takes.
    pub(super) fn built_in_named(&self, cmd: &Command) -> Option<&'static crate::services::styles::builtin::BuiltIn> {
        let (id, kind) = match cmd {
            Command::SetParagraphStyle(id) => (id, Some(StyleKind::Paragraph)),
            Command::ApplyCharacterStyle(Some(id)) => (id, Some(StyleKind::Character)),
            Command::SetTableStyle(id) => (id, Some(StyleKind::Table)),
            Command::UpdateStyleToSelection(id) => (id, None),
            _ => return None,
        };
        let styles = &self.doc.resolver.as_ref()?.styles;
        if styles.by_id.contains_key(&id.0) {
            return None;
        }
        crate::services::styles::builtin::BUILT_IN.iter().find(|b| b.id == id.0 && kind.is_none_or(|k| k == b.kind))
    }

    /// Whether `cmd` names a built-in style the document will take on its first use.
    pub fn built_in_for(&self, cmd: &Command) -> bool {
        self.built_in_named(cmd).is_some()
    }

    /// Adds built-in style `b` from its definition, as one undo step.
    pub(super) fn add_built_in(&mut self, b: &crate::services::styles::builtin::BuiltIn, fonts: &Fonts) -> Outcome {
        let Some(cx) = self.doc.resolver.as_ref().map(|r| r.styles.cx()) else { return Outcome::Unchanged("no styles") };
        let w = cx.w.clone();
        let mut raw = crate::services::styles::builtin::definition(b, &w);
        // List Bullet comes with its own bullet list.
        if b.id == "ListBullet" {
            use crate::docx::write::props::Props;
            let mut tx = self.begin("New Style");
            let num = self.new_style_bullets(&mut tx, b.id);
            if num == 0 {
                self.abort(tx);
                return Outcome::Unchanged("the list couldn't be added");
            }
            self.commit(tx, None, fonts);
            let mut style = Props::parse(Some(&raw), "style", &cx);
            let mut ppr = Props::parse(style.get("pPr"), "pPr", &cx);
            ppr.set("numPr", format!("<{w}numPr><{w}numId {w}val=\"{num}\"/></{w}numPr>"));
            ppr.set("contextualSpacing", format!("<{w}contextualSpacing/>"));
            if let Some(x) = ppr.xml() {
                style.set("pPr", x);
            }
            if let Some(x) = style.xml() {
                raw = x;
            }
        }
        self.edit_styles("New Style", |s| s.set_xml(raw), fonts)
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
        let shadow = b.shadow.unwrap_or(false);
        let line = |v: Option<Option<crate::api::command::Border>>, far: bool| {
            v.map(|v| {
                v.map(|v| super::readback::BorderLine {
                    style: v.style,
                    width: v.width,
                    color: super::commands::rgb_value(v.color),
                    space: crate::api::value::Points(1.0),
                    shadow: shadow && far,
                })
            })
        };
        let edges = super::readback::ParagraphBorderEdges {
            top: line(b.top, false),
            left: line(b.left, false),
            bottom: line(b.bottom, true),
            right: line(b.right, true),
            between: line(b.between, false),
        };
        self.set_paragraph_border_edges(&edges, fonts)
    }

    /// Sets the selected paragraphs' border edges, each set, removed or kept: its line, colour (RGB or a theme colour),
    /// distance from the text and shadow.
    pub fn set_paragraph_border_edges(&mut self, b: &super::readback::ParagraphBorderEdges, fonts: &Fonts) -> Outcome {
        let edges = [("top", &b.top), ("left", &b.left), ("bottom", &b.bottom), ("right", &b.right), ("between", &b.between)];
        let mut colours = [None; 5];
        for (k, (_, e)) in edges.iter().enumerate() {
            if let Some(Some(line)) = e {
                match super::readback::checked(self, line) {
                    Ok(c) => colours[k] = c,
                    Err(why) => return Outcome::invalid(&why),
                }
            }
        }
        self.format_paras(
            "Borders",
            &|p, cx, _| {
                let mut bdr = Props::parse(p.get("pBdr"), "pBdr", cx);
                bdr.reorder(&["top", "left", "bottom", "right", "between", "bar"]);
                for (k, (side, v)) in edges.iter().enumerate() {
                    match v {
                        Some(Some(v)) => bdr.set(side, v.xml(&cx.w, side, colours[k])),
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
        let line = b.border.map(|l| super::readback::BorderLine {
            style: l.style,
            width: l.width,
            color: super::commands::rgb_value(l.color),
            space: crate::api::value::Points(0.0),
            shadow: b.shadow,
        });
        self.set_text_border_line(line.as_ref(), fonts)
    }

    /// Sets the selected text's border: one box around it, its line, colour (RGB or a theme colour), distance from the
    /// text and shadow; none removes it.
    pub fn set_text_border_line(&mut self, b: Option<&super::readback::BorderLine>, fonts: &Fonts) -> Outcome {
        let colour = match b.map(|b| super::readback::checked(self, b)).transpose() {
            Ok(c) => c.flatten(),
            Err(why) => return Outcome::invalid(&why),
        };
        self.format_runs(
            "Borders",
            &|p, w, _| match b {
                Some(b) => p.set("bdr", b.xml(w, "bdr", colour)),
                None => p.remove("bdr"),
            },
            fonts,
        )
    }

    /// Sets the shading of the selected paragraphs, text, cells or tables.
    pub(super) fn set_shading(&mut self, s: crate::api::command::ShadingChange, fonts: &Fonts) -> Outcome {
        use crate::api::drawing::ColourValue;
        use crate::api::value::NoneOr;
        let v = super::readback::ShadingValue {
            fill: match s.fill {
                NoneOr::Value(c) => NoneOr::Value(ColourValue::rgb(c)),
                NoneOr::None => NoneOr::None,
            },
            pattern: s.pattern,
            color: super::commands::rgb_value(s.color),
        };
        self.set_shading_value(s.target, &v, fonts)
    }

    /// Sets the shading of the selected paragraphs, text, cells or tables: its fill, pattern and pattern colour, each
    /// an RGB or a theme colour with a tint or shade.
    pub fn set_shading_value(&mut self, target: crate::api::command::ShadingTarget, s: &super::readback::ShadingValue, fonts: &Fonts) -> Outcome {
        use super::table_format::{Shading, TableFormat};
        use crate::api::command::{ShadingPattern, ShadingTarget};
        use crate::api::value::{AutoOr, NoneOr};
        let colour = |c: &crate::api::drawing::ColourValue| self.word_colour(c);
        let fill = match &s.fill {
            NoneOr::Value(c) => match colour(c) {
                Ok(c) => Some(c),
                Err(why) => return Outcome::invalid(&why),
            },
            NoneOr::None => None,
        };
        let color = match &s.color {
            AutoOr::Auto => None,
            AutoOr::Value(c) => match colour(c) {
                Ok(c) => Some(c),
                Err(why) => return Outcome::invalid(&why),
            },
        };
        let sh = Shading { pattern: s.pattern.val(), color, fill };
        // No fill and a clear pattern: no shading of their own.
        let none = fill.is_none() && s.pattern == ShadingPattern::Clear;
        let edit = |p: &mut Props, w: &str, inherited: Option<&crate::docx::props::Shd>| {
            if none {
                p.remove("shd");
            } else {
                let x = sh.xml(w, p.get("shd"), inherited);
                p.set("shd", x);
            }
        };
        // A table style's shading goes in its cells' or its table's properties; no other style has them, and the
        // document's tables stay as they are.
        if let Some(raw) = &self.style_target
            && matches!(target, ShadingTarget::Cell | ShadingTarget::Table)
        {
            let cx = self.doc.resolver.as_ref().map(|r| r.styles.cx());
            if !cx.and_then(|cx| cx.parse(raw, |_, n| crate::docx::xml::attr(n, "type") == Some("table"))).unwrap_or(false) {
                return Outcome::Refused(Error::invalid("only a table style has cell or table shading"));
            }
            let (part, order) = match target {
                ShadingTarget::Cell => ("tcPr", crate::docx::write::props::TCPR),
                _ => ("tblPr", crate::docx::write::props::TBLPR),
            };
            return self.format_style_part(part, &|p, cx, _, _| {
                p.reorder(order);
                edit(p, &cx.w, None);
            });
        }
        match target {
            ShadingTarget::Paragraph => self.format_paras("Shading", &|p, cx, para| edit(p, &cx.w, para.style.shd.as_ref()), fonts),
            ShadingTarget::Text => self.format_runs("Shading", &|p, w, run| edit(p, w, run.shd.as_ref()), fonts),
            ShadingTarget::Cell => self.table_format(&TableFormat::CellShading(sh.clone()), fonts),
            ShadingTarget::Table => self.table_format(&TableFormat::TableShading(sh.clone()), fonts),
        }
    }

    /// Sets the selected cells' fill (clear pattern), or removes their shading: an RGB or a theme colour with a tint
    /// or shade.
    pub fn set_cell_shading(&mut self, c: Option<crate::api::drawing::ColourValue>, fonts: &Fonts) -> Outcome {
        if let Some(out) = self.refuse_like(&Command::SetCellShading(None)) {
            return out;
        }
        let fill = match c.map(|c| self.word_colour(&c)).transpose() {
            Ok(f) => f,
            Err(why) => return Outcome::invalid(&why),
        };
        self.table_format(&super::table_format::TableFormat::CellShading(super::table_format::Shading::fill(fill)), fonts)
    }

    /// Deletes a style the document made.
    pub(super) fn delete_style(&mut self, id: &str, fonts: &Fonts) -> Outcome {
        let Some(style) = self.doc.resolver.as_ref().and_then(|r| r.styles.get(id)).cloned() else {
            return Outcome::Refused(Error::unavailable("no such style"));
        };
        if !style.custom || self.doc.resolver.as_ref().is_some_and(|r| r.styles.default_paragraph.as_deref() == Some(id)) {
            return Outcome::Refused(Error::new(crate::api::error::ErrorCode::NotAllowed, "built-in styles can't be deleted"));
        }
        self.edit_styles("Delete Style", |s| s.delete(id), fonts)
    }

    /// The paragraphs, marks, runs (a ruby's too) and tables whose direct formatting names style `id`, with the name
    /// taken out, as stale.
    fn drop_style_refs(&mut self, tx: &mut super::Tx, id: &str, res: &crate::docx::resolve::Resolver) {
        use crate::docx::Inline;
        use crate::docx::write::props::attr_of;
        let cx = self.cx();
        // The XML without `name` when it names the style
        let without = |xml: Option<&str>, local: &str, name: &str| -> Option<Option<String>> {
            let mut p = Props::parse(xml, local, &cx);
            p.get(name).and_then(|x| attr_of(x, "val")).is_some_and(|v| v == id).then(|| {
                p.remove(name);
                p.xml()
            })
        };
        let raw = |x: Option<String>| crate::docx::Raw(x.map(|x| Rc::from(x.as_str())));
        let named = |r: &crate::docx::Run| r.style_id.as_deref() == Some(id);
        let unstyled = |r: &crate::docx::Run| -> Option<crate::docx::Run> {
            without(r.direct.0.as_deref(), "rPr", "rStyle").map(|x| Rc::new(RunStyle { direct: raw(x), ..(**r).clone() }))
        };
        let in_ruby = |i: &Inline| matches!(i, Inline::Ruby(r, _) if r.base.iter().chain(&r.text).any(|x| named(&x.1)));
        let ruby_rpr = |xml: Option<&str>, _: &RunStyle| without(xml, "rPr", "rStyle").unwrap_or_else(|| xml.map(str::to_string));
        let paras: Vec<_> = self.index.para_ids().collect();
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let in_style = p.style.style_id.as_deref() == Some(id);
            if !in_style && !named(&p.mark) && !p.inlines.iter().any(|i| i.run().is_some_and(named) || in_ruby(i)) {
                continue;
            }
            let style = in_style
                .then(|| without(p.style.direct.0.as_deref(), "pPr", "pStyle"))
                .flatten()
                .map(|x| Rc::new(crate::docx::ParaStyle { direct: raw(x), ..(*p.style).clone() }));
            let mark = if named(&p.mark) { unstyled(&p.mark) } else { None };
            let mut inlines: Option<Vec<Inline>> = None;
            let mut at = 0;
            for (k, i) in p.inlines.iter().enumerate() {
                let unit = crate::layout::Unit(at);
                at += i.units() as u32;
                let run = i.run().filter(|r| named(r)).and_then(unstyled);
                let mut edited = run.is_some();
                let mut with = run.map_or_else(|| i.clone(), |r| super::format::with_run(i, &r));
                if in_ruby(i)
                    && let Inline::Ruby(r, _) = &mut with
                    && let Some(base) = self.run_base(para, unit, res, &cx)
                    && let Some(n) = super::format::ruby_again(r, &base, Some((&ruby_rpr, true)), res, &cx)
                {
                    (*r, edited) = (n, true);
                }
                if edited {
                    inlines.get_or_insert_with(|| p.inlines.clone())[k] = with;
                }
            }
            if style.is_some() || mark.is_some() {
                let (style, mark, list) = (style.unwrap_or_else(|| p.style.clone()), mark.unwrap_or_else(|| p.mark.clone()), p.list.clone());
                let _ = self.step(tx, Step::Para { para, style, mark, list });
            }
            if let Some(inlines) = inlines {
                let _ = self.step(tx, Step::Restyle { para, inlines });
            }
            tx.stale_paras.push(para);
        }
        let tables: Vec<_> = self.index.table_ids().collect();
        for table in tables {
            let Some(t) = self.index.table(&self.doc, table).filter(|t| t.style_id.as_deref() == Some(id)) else { continue };
            if let Some(x) = without(t.direct.0.as_deref(), "tblPr", "tblStyle") {
                let mut props = super::step::TableProps::of(t);
                props.direct = raw(x);
                let _ = self.step(tx, Step::TableProps { table, props });
            }
            tx.stale_tables.push(table);
        }
    }

    /// Sets a style's list flags.
    pub(super) fn set_style_meta(&mut self, m: crate::api::command::StyleMetaChange, fonts: &Fonts) -> Outcome {
        use crate::api::services::StyleVisibility;
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let Some(style) = res.styles.get(&m.id.0) else { return self.set_latent_meta(&m, fonts) };
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

    /// The document defaults as a paragraph style's XML: its `w:pPr` and `w:rPr`.
    fn defaults_stand_in(&self) -> Option<String> {
        let res = self.doc.resolver.as_ref()?;
        let cx = res.styles.cx();
        let w = cx.w.clone();
        let defaults = res.styles.defaults_raw.clone().unwrap_or_else(|| format!("<{w}docDefaults/>"));
        let parts = Props::parse(Some(&defaults), "docDefaults", &cx);
        let inner = |wrapper: &str, part: &str| {
            parts.get(wrapper).and_then(|x| Props::parse(Some(x), wrapper, &cx).get(part).map(str::to_string)).unwrap_or_default()
        };
        Some(format!(
            "<{w}style {w}type=\"paragraph\" {w}styleId=\"DocumentDefaults\"><{w}name {w}val=\"Document Defaults\"/>{}{}</{w}style>",
            inner("pPrDefault", "pPr"),
            inner("rPrDefault", "rPr")
        ))
    }

    /// The document's default formatting, without a dialog: text before any style's (resolved), and what the
    /// defaults set themselves (the rest unavailable: the application's apply).
    pub fn document_defaults(&self) -> Option<super::style_defs::DefaultsDefinition> {
        let raw = self.defaults_stand_in()?;
        let own = self.doc.resolver.as_ref()?.styles.style_from_xml(&raw)?;
        // Text in a style that sets nothing: the defaults alone.
        let w = self.doc.resolver.as_ref()?.styles.cx().w;
        let (character, paragraph, tabs) = self.style_formatting_state(&format!(
            "<{w}style {w}type=\"paragraph\" {w}styleId=\"DocumentDefaults\"><{w}name {w}val=\"Document Defaults\"/></{w}style>"
        ))?;
        Some(super::style_defs::DefaultsDefinition {
            own_character: super::style_defs::own_character(&own.base.rpr, &character),
            own_paragraph: super::style_defs::own_paragraph(&own.base.ppr, &paragraph),
            character,
            paragraph,
            tabs,
        })
    }

    /// Sets the document's default formatting: the formatting commands run on the defaults as if they were a style.
    pub(super) fn set_document_defaults(&mut self, d: crate::api::command::DocumentDefaults, fonts: &Fonts) -> Outcome {
        self.set_document_defaults_with(d, None, None, fonts)
    }

    /// [`Editor::set_document_defaults`], the font a theme font (`theme_font`, over `d.font`) and the colour a theme
    /// colour with a tint or shade (`colour`, over `d.color`).
    pub fn set_document_defaults_with(
        &mut self,
        d: crate::api::command::DocumentDefaults,
        theme_font: Option<crate::docx::ThemeFont>,
        colour: Option<crate::api::value::AutoOr<crate::api::drawing::ColourValue>>,
        fonts: &Fonts,
    ) -> Outcome {
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
        self.format_defaults(commands, theme_font, colour, fonts)
    }

    /// Sets the document's default formatting with any character and paragraph formatting commands but frames, run
    /// on the defaults as on a style, as one undo step. Every style's text lays out again.
    pub fn set_default_formatting(&mut self, commands: Vec<Command>, fonts: &Fonts) -> Outcome {
        use crate::api::command::CommandId as C;
        if let Some(c) = commands.iter().find(|c| matches!(c.id(), C::SetFrame | C::RemoveFrame)) {
            return Outcome::invalid(&format!("{:?} doesn't set a default", c.id()));
        }
        self.format_defaults(commands, None, None, fonts)
    }

    /// Formats the defaults with `commands`, then a theme font and a theme colour, as one undo step.
    fn format_defaults(
        &mut self,
        commands: Vec<Command>,
        theme_font: Option<crate::docx::ThemeFont>,
        colour: Option<crate::api::value::AutoOr<crate::api::drawing::ColourValue>>,
        fonts: &Fonts,
    ) -> Outcome {
        if let Some(out) = self.refuse_like(&Command::SetDocumentDefaults(Default::default())) {
            return out;
        }
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let Some(stand_in) = self.defaults_stand_in() else { return Outcome::Unchanged("no styles") };
        let cx = res.styles.cx();
        let w = cx.w.clone();
        let defaults = res.styles.defaults_raw.clone().unwrap_or_else(|| format!("<{w}docDefaults/>"));
        let parts = Props::parse(Some(&defaults), "docDefaults", &cx);
        let inner = |wrapper: &str, part: &str| {
            parts.get(wrapper).and_then(|x| Props::parse(Some(x), wrapper, &cx).get(part).map(str::to_string)).unwrap_or_default()
        };
        self.defaults_target = true;
        let mut edited = self.format_style(&stand_in, commands, fonts);
        if let Ok(x) = &mut edited
            && (theme_font.is_some() || colour.is_some())
        {
            self.style_target = Some(std::mem::take(x));
            let font = theme_font.map(|f| self.set_theme_font(f, fonts));
            let colour = colour.map(|c| self.set_font_colour(c, fonts));
            *x = self.style_target.take().unwrap_or_default();
            if let Some(Outcome::Refused(e)) = [font, colour].into_iter().flatten().find(|o| matches!(o, Outcome::Refused(_))) {
                edited = Err(e);
            }
        }
        self.defaults_target = false;
        let edited = match edited {
            Ok(x) => x,
            Err(e) => return Outcome::Refused(e),
        };
        let el = Props::parse(Some(&edited), "style", &cx);
        if [("pPrDefault", "pPr"), ("rPrDefault", "rPr")].iter().all(|&(wrapper, part)| el.get(part).unwrap_or_default() == inner(wrapper, part)) {
            return Outcome::Unchanged("already so");
        }
        // The defaults as written with their properties replaced: a wrapper read empty stays (no wrapper means the
        // application's defaults).
        let mut out = Props::parse(Some(&defaults), "docDefaults", &cx);
        for (wrapper, part) in [("rPrDefault", "rPr"), ("pPrDefault", "pPr")] {
            let mut wrap = Props::parse(parts.get(wrapper), wrapper, &cx);
            match el.get(part) {
                Some(x) => wrap.set(part, x.to_string()),
                None => wrap.remove(part),
            }
            if parts.get(wrapper).is_some() || el.get(part).is_some() {
                out.set(wrapper, wrap.xml().unwrap_or_else(|| format!("<{w}{wrapper}/>")));
            }
        }
        out.reorder(&["rPrDefault", "pPrDefault"]);
        let raw = out.xml().unwrap_or_else(|| format!("<{w}docDefaults/>"));
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
        self.style_target = Some(raw.clone());
        let _ = self.format_style_part("pPr", &|p, cx, para, _| edit(p, cx, para));
        let Some(now) = self.style_target.take() else { return Outcome::Unchanged("no style") };
        // The formatting it has already, as for paragraphs formatted directly: no undo step.
        if now == raw || self.style_formatting(&now).zip(self.style_formatting(&raw)).is_some_and(|(a, b)| a.0.style == b.0.style) {
            return Outcome::Unchanged("the style already has it");
        }
        self.edit_styles("Modify Style", |s| s.set_xml(now), fonts)
    }

    /// The paragraphs in one of the styles `changed`, or with a run in one: its mark, its list label, a ruby's text
    /// too.
    pub(super) fn styled_paras(&self, changed: &[String]) -> Vec<crate::layout::ParaId> {
        let default = self.doc.resolver.as_ref().and_then(|r| r.styles.default_paragraph.clone());
        let changed: std::collections::HashSet<&str> = changed.iter().map(String::as_str).collect();
        let uses = |id: Option<&str>| id.or(default.as_deref()).is_some_and(|id| changed.contains(id));
        let styled = |r: &crate::docx::Run| r.style_id.as_deref().is_some_and(|s| changed.contains(s));
        self.index
            .para_ids()
            .filter(|&p| {
                self.index.para(&self.doc, p).is_some_and(|q| {
                    uses(q.style.style_id.as_deref())
                        || styled(&q.mark)
                        || q.list.as_ref().is_some_and(|l| styled(&l.run))
                        || q.inlines.iter().any(|i| {
                            i.run().is_some_and(styled)
                                || matches!(i, crate::docx::Inline::Ruby(r, _) if r.base.iter().chain(&r.text).any(|x| styled(&x.1)))
                        })
                })
            })
            .collect()
    }

    /// Runs `edit` on the styles table as one undo step named `label`. Every paragraph, run and table whose style
    /// changed (the edit's answer) is formatted again from the new table; those naming a style it deleted name it no
    /// more.
    pub(crate) fn edit_styles(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut Styles) -> Result<Vec<String>, StyleEditError>,
        fonts: &Fonts,
    ) -> Outcome {
        self.edit_styles_relabel(label, edit, false, fonts)
    }

    /// [`Self::edit_styles`], and with `relabel` every list paragraph is numbered again: an edit that changes the
    /// lists styles number with.
    pub(crate) fn edit_styles_relabel(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut Styles) -> Result<Vec<String>, StyleEditError>,
        relabel: bool,
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
        next.compute_style_links();
        let mut tx = self.begin(label);
        let added = self.doc.added_lists.clone();
        if self.step(&mut tx, Step::Resolver { resolver: Some(Rc::new(next)), added }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the styles didn't change");
        }
        // The styles it deleted: the document's references to them go, untracked
        let gone: Vec<&str> = res
            .styles
            .by_id
            .keys()
            .filter(|id| !self.doc.resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key(*id)))
            .map(String::as_str)
            .collect();
        let tracked = std::mem::replace(&mut tx.tracked, false);
        for id in gone {
            self.drop_style_refs(&mut tx, id, &res);
        }
        tx.tracked = tracked;
        let paras = self.styled_paras(&changed);
        let tables: Vec<_> = self
            .index
            .table_ids()
            .filter(|&t| self.index.table(&self.doc, t).and_then(|t| t.style_id.as_deref()).is_some_and(|s| changed.iter().any(|c| c == s)))
            .collect();
        tx.stale_paras.extend(paras);
        tx.stale_tables.extend(tables);
        if relabel {
            let lists: Vec<crate::layout::ParaId> = self.list_paras().into_iter().map(|(p, _, _)| p).collect();
            tx.stale_paras.extend(lists);
            tx.relabel = true;
        }
        self.commit(tx, None, fonts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_style_part_is_edited_in_place_however_its_type_is_quoted() {
        let cx = Cx::of_xml(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#);
        let raw = r#"<w:style w:type="table" w:styleId="T"><w:name w:val="T"/><w:tblStylePr w:type='firstRow'><w:tcPr><w:noWrap/></w:tcPr></w:tblStylePr></w:style>"#;
        let xml = edit_table_part(raw, "firstRow", "tcPr", &cx, |p| p.set("vAlign", r#"<w:vAlign w:val="center"/>"#.into()));
        assert_eq!(xml.matches("<w:tblStylePr").count(), 1, "{xml}");
        assert!(xml.contains(r#"<w:tcPr><w:noWrap/><w:vAlign w:val="center"/></w:tcPr>"#), "{xml}");
    }
}
