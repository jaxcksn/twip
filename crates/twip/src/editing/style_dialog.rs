//! The style dialog

use crate::api::command::{Command, Indents, ParagraphSpacing};
use crate::api::error::Error;
use crate::api::services::{StyleDialog, StyleDialogType, StyleOpen, TableStylePart};
use crate::api::state::{CharacterFormatting, ParagraphFormatting, Tri};
use crate::api::value::{AutoOr, Color, NoneOr, StyleId};
use crate::docx::styles::{Style, StyleKind, Styles};
use crate::docx::write::props::{Cx, Props, STYLE, esc};
use crate::font::Fonts;
use crate::services::styles::{NameRefusal, check_name, describe, next_new_name};

use super::Editor;
use super::commands::Outcome;

fn one<T: Clone + Default>(t: &Tri<T>) -> T {
    match t {
        Tri::Uniform(v) => v.clone(),
        _ => T::default(),
    }
}

/// The style a style's XML holds, read with the rest of `styles` around it.
fn parsed(styles: &Styles, raw: &str) -> Option<Style> {
    let mut s = styles.clone();
    s.set_xml(raw.to_string()).ok()?;
    s.by_id.into_values().find(|st| st.raw == raw)
}

/// The XML's style element with one child set, or removed when `xml` is none.
fn set_child(raw: &str, cx: &Cx, name: &str, xml: Option<String>) -> String {
    let mut el = Props::parse(Some(raw), "style", cx);
    match xml {
        Some(x) => el.set(name, x),
        None => el.remove(name),
    }
    el.reorder(STYLE);
    el.xml().unwrap_or_else(|| raw.to_string())
}

/// The XML with its start tag replaced.
fn with_start(raw: &str, start: &str) -> String {
    raw.find('>').map_or_else(|| raw.to_string(), |e| format!("{start}{}", &raw[e + 1..]))
}

/// The `w:type` a style of a dialog type is.
fn xml_type(kind: StyleDialogType) -> &'static str {
    match kind {
        StyleDialogType::Character => "character",
        StyleDialogType::Table => "table",
        StyleDialogType::List => "numbering",
        _ => "paragraph",
    }
}

/// A table style's XML with the dialog's fill, cell alignment, border and band sizes written where they changed: for
/// the part `region`, or the whole table.
fn table_style_edits(raw: &str, was: &StyleDialog, now: &StyleDialog, region: Option<&'static str>, cx: &Cx) -> String {
    use crate::docx::write::props::{TBLPR, TCPR};
    let w = cx.w.clone();
    let mut raw = raw.to_string();
    let root = |raw: &str, child: &str, order: &'static [&'static str], edit: &dyn Fn(&mut Props)| {
        super::styles::edit_child(raw, child, cx, |p| {
            p.reorder(order);
            edit(p);
        })
    };
    let cell = |raw: &str, edit: &dyn Fn(&mut Props)| match region {
        Some(r) => super::styles::edit_table_part(raw, r, "tcPr", cx, edit),
        None => root(raw, "tcPr", TCPR, edit),
    };
    if now.fill != was.fill {
        raw = cell(&raw, &|p| match now.fill {
            Some(c) => p.set("shd", format!("<{w}shd {w}val=\"clear\" {w}color=\"auto\" {w}fill=\"{:06X}\"/>", c.0)),
            None => p.remove("shd"),
        });
    }
    if now.cell_alignment != was.cell_alignment {
        let v = match now.cell_alignment {
            crate::api::value::CellAlignment::Top => "top",
            crate::api::value::CellAlignment::Center => "center",
            crate::api::value::CellAlignment::Bottom => "bottom",
        };
        raw = cell(&raw, &|p| p.set("vAlign", format!("<{w}vAlign {w}val=\"{v}\"/>")));
    }
    if now.border != was.border {
        let edges = |name: &str| -> String {
            let one = |side: &str| match &now.border {
                Some(b) => {
                    let color = match b.color {
                        AutoOr::Auto => "auto".to_string(),
                        AutoOr::Value(c) => format!("{:06X}", c.0),
                    };
                    format!(
                        "<{w}{side} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"0\" {w}color=\"{color}\"/>",
                        b.style.val(),
                        (b.width.0 * 8.0).round() as i64
                    )
                }
                None => format!("<{w}{side} {w}val=\"nil\"/>"),
            };
            let sides: String = ["top", "left", "bottom", "right", "insideH", "insideV"].iter().map(|s| one(s)).collect();
            format!("<{w}{name}>{sides}</{w}{name}>")
        };
        raw = match region {
            Some(r) => super::styles::edit_table_part(&raw, r, "tcPr", cx, |p| p.set("tcBorders", edges("tcBorders"))),
            None => root(&raw, "tblPr", TBLPR, &|p| p.set("tblBorders", edges("tblBorders"))),
        };
    }
    if now.row_band != was.row_band || now.column_band != was.column_band {
        let (rows, cols) = (now.row_band.max(1), now.column_band.max(1));
        raw = root(&raw, "tblPr", TBLPR, &|p| {
            p.set("tblStyleRowBandSize", format!("<{w}tblStyleRowBandSize {w}val=\"{rows}\"/>"));
            p.set("tblStyleColBandSize", format!("<{w}tblStyleColBandSize {w}val=\"{cols}\"/>"));
        });
    }
    raw
}

/// A style id from a name: its letters and digits.
fn id_for(name: &str, styles: &Styles) -> String {
    let base: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
    let base = if base.is_empty() { "Style".to_string() } else { base };
    (0..).map(|n| if n == 0 { base.clone() } else { format!("{base}{n}") }).find(|id| !styles.by_id.contains_key(id)).unwrap_or(base)
}

impl Editor {
    /// The table the selection is in.
    fn selection_table(&self) -> Option<crate::layout::TableId> {
        if let super::selection::Selection::Cells { table, .. } = self.selection {
            return Some(table);
        }
        let focus = self.selection.focus()?;
        self.index.chain_of(&focus).last().map(|l| l.table)
    }

    /// Opens the style dialog: on a style, or on a new style from the selection's formatting.
    pub fn style_dialog_open(&self, open: StyleOpen) -> Result<StyleDialog, Error> {
        let res = self.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?;
        let styles = &res.styles;
        let cx = styles.cx();
        let w = &cx.w;
        let pending = match &open {
            StyleOpen::Modify(id) => styles.get(&id.0).ok_or_else(|| Error::unavailable("no such style"))?.raw.clone(),
            StyleOpen::Create => {
                let name = next_new_name(styles);
                let para = self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para(&self.doc, p));
                let base = para.and_then(|p| p.style.style_id.clone()).or_else(|| styles.default_paragraph.clone());
                // The selection's direct formatting: its paragraph's, and its first run's.
                let ppr = para.and_then(|p| p.style.direct.0.as_deref()).map(|x| {
                    let mut p = Props::parse(Some(x), "pPr", &cx);
                    for k in ["pStyle", "numPr", "sectPr", "rPr", "pPrChange"] {
                        p.remove(k);
                    }
                    p.xml().unwrap_or_default()
                });
                let rpr = self.caret().and_then(|c| self.run_at(&c)).and_then(|r| r.direct.0.as_deref().map(str::to_string)).map(|x| {
                    let mut p = Props::parse(Some(&x), "rPr", &cx);
                    for k in ["rStyle", "ins", "del", "rPrChange"] {
                        p.remove(k);
                    }
                    p.xml().unwrap_or_default()
                });
                let base = base.map_or(String::new(), |b| format!("<{w}basedOn {w}val=\"{}\"/>", esc(&b)));
                format!(
                    "<{w}style {w}type=\"paragraph\" {w}customStyle=\"1\" {w}styleId=\"{id}\"><{w}name {w}val=\"{n}\"/>{base}<{w}next {w}val=\"{id}\"/><{w}qFormat/>{}{}</{w}style>",
                    ppr.unwrap_or_default(),
                    rpr.unwrap_or_default(),
                    id = esc(&id_for(&name, styles)),
                    n = esc(&name)
                )
            }
        };
        let short_form = open == StyleOpen::Create;
        let mut v = self.style_values(&pending, open, styles, TableStylePart::WholeTable)?;
        v.short_form = short_form;
        Ok(v)
    }

    /// The dialog's values for a style's XML: its name, type and flags, its formatting as text in it resolves, and
    /// its description.
    fn style_values(&self, pending: &str, open: StyleOpen, styles: &Styles, part: TableStylePart) -> Result<StyleDialog, Error> {
        let style = parsed(styles, pending).ok_or_else(|| Error::invalid("not a style"))?;
        let res = self.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?;
        let mut next = (**res).clone();
        let _ = next.styles.set_xml(pending.to_string());
        // A table style shows its part's formatting: the whole table's, with the part's over it.
        let region =
            (style.kind == StyleKind::Table).then(|| next.styles.table_region(Some(&style.id), &part.region().into_iter().collect::<Vec<_>>()));
        let (c, p): (CharacterFormatting, ParagraphFormatting) = match &region {
            Some(region) => {
                let cx = next.styles.cx();
                let para = next.para_from_xml(None, Some(region), &cx);
                let run = next.run_from_xml(&para.base, None, &cx);
                (super::state::character(&[&run]), super::state::paragraph(&[(&para.style, None)]))
            }
            None => {
                let (para, run, _) = self.style_formatting(pending).ok_or_else(|| Error::invalid("not a style"))?;
                (super::state::character(&[&run]), super::state::paragraph(&[(&para.style, None)]))
            }
        };
        let whole = (style.kind == StyleKind::Table).then(|| next.styles.table_region(Some(&style.id), &[]));
        let line = |b: &Option<crate::docx::props::Border>| super::borders_dialog::line(b);
        let border = match (&region, part) {
            (Some(_), TableStylePart::WholeTable) => whole.as_ref().and_then(|r| line(&r.tbl.borders.top)),
            (Some(r), _) => line(&r.tc.borders.top),
            _ => None,
        };
        let list_levels = match style.kind {
            StyleKind::Numbering => {
                let num = next.styles.chain_ppr(Some(&style.id)).num_id.filter(|n| *n != 0);
                num.and_then(|n| self.list_levels(n)).unwrap_or_else(|| super::list_defs::complete(&[]))
            }
            _ => Vec::new(),
        };
        let points = |t: &Tri<AutoOr<crate::api::value::Points>>| match t {
            Tri::Uniform(AutoOr::Value(v)) => *v,
            _ => crate::api::value::Points(0.0),
        };
        let mut with = styles.clone();
        let _ = with.set_xml(pending.to_string());
        Ok(StyleDialog {
            open,
            short_form: false,
            name: std::iter::once(crate::services::styles::describe::display_name(&style))
                .chain(style.aliases.iter().cloned())
                .collect::<Vec<_>>()
                .join(","),
            kind: match style.kind {
                StyleKind::Paragraph if style.link.is_some() => StyleDialogType::Linked,
                StyleKind::Paragraph => StyleDialogType::Paragraph,
                StyleKind::Character => StyleDialogType::Character,
                StyleKind::Table => StyleDialogType::Table,
                StyleKind::Numbering => StyleDialogType::List,
            },
            based_on: style.based_on.clone().map(StyleId),
            next: style.next.clone().map(StyleId),
            font: one(&c.font),
            size: one(&c.size),
            bold: one(&c.bold),
            italic: one(&c.italic),
            underline: one(&c.underline),
            color: one(&c.color),
            alignment: one(&p.alignment),
            line_spacing: one(&p.line_spacing),
            space_before: points(&p.space_before),
            space_after: points(&p.space_after),
            left_indent: one(&p.indents.left),
            quick: style.quick,
            auto_update: style.meta.auto_redefine,
            table_part: part,
            fill: region.as_ref().and_then(|r| r.tc.shading.flatten()).map(crate::api::value::Color),
            cell_alignment: match region.as_ref().and_then(|r| r.tc.v_align.as_deref()) {
                Some("center") => crate::api::value::CellAlignment::Center,
                Some("bottom") => crate::api::value::CellAlignment::Bottom,
                _ => crate::api::value::CellAlignment::Top,
            },
            border,
            row_band: whole.as_ref().and_then(|r| r.tbl.row_band).unwrap_or(1).max(1) as u32,
            column_band: whole.as_ref().and_then(|r| r.tbl.col_band).unwrap_or(1).max(1) as u32,
            list_levels,
            list_level: 0,
            description: with.by_id.values().find(|s| s.raw == pending).map(|s| describe::describe(s, &with)).unwrap_or_default(),
            pending: pending.to_string(),
        })
    }

    /// The style dialog after the user changed `now` from `was`: the formatting it shows set on the style, the
    /// name, base, following style and flags written into it, and the values read again.
    pub fn style_dialog_update(&mut self, was: &StyleDialog, now: StyleDialog, fonts: &Fonts) -> Result<StyleDialog, Error> {
        let res = self.doc.resolver.clone().ok_or_else(|| Error::unavailable("no styles"))?;
        let cx = res.styles.cx();
        let w = cx.w.clone();
        let para = matches!(now.kind, StyleDialogType::Paragraph | StyleDialogType::Linked);
        let table = now.kind == StyleDialogType::Table;
        let mut commands = Vec::new();
        if now.font != was.font && !now.font.trim().is_empty() {
            commands.push(Command::SetFont(now.font.clone()));
        }
        if now.size != was.size {
            commands.push(Command::SetFontSize(now.size));
        }
        if now.bold != was.bold {
            commands.push(Command::SetBold(now.bold));
        }
        if now.italic != was.italic {
            commands.push(Command::SetItalic(now.italic));
        }
        if now.underline != was.underline {
            commands.push(Command::SetUnderline(now.underline));
        }
        if now.color != was.color {
            commands.push(Command::SetFontColor(now.color));
        }
        if (para || table) && now.alignment != was.alignment {
            commands.push(Command::SetAlignment(now.alignment));
        }
        if para {
            if now.line_spacing != was.line_spacing {
                commands.push(Command::SetLineSpacing(now.line_spacing));
            }
            if now.space_before != was.space_before || now.space_after != was.space_after {
                let before = (now.space_before != was.space_before).then_some(AutoOr::Value(now.space_before));
                let after = (now.space_after != was.space_after).then_some(AutoOr::Value(now.space_after));
                commands.push(Command::SetParagraphSpacing(ParagraphSpacing { before, after }));
            }
            if now.left_indent != was.left_indent {
                commands.push(Command::SetIndents(Indents { left: Some(now.left_indent), right: None, first_line: None, hanging: None }));
            }
        }
        // A table style's part: the formatting commands, fill, alignment and border edit it.
        let region = if table { was.table_part.region() } else { None };
        self.style_part = region;
        let formatted = if commands.is_empty() { Ok(was.pending.clone()) } else { self.format_style(&was.pending, commands, fonts) };
        self.style_part = None;
        let mut pending = formatted?;
        if table {
            pending = table_style_edits(&pending, was, &now, region, &cx);
        }
        let id_ref = |id: &StyleId| esc(&id.0).to_string();
        if now.based_on != was.based_on {
            pending = set_child(&pending, &cx, "basedOn", now.based_on.as_ref().map(|b| format!("<{w}basedOn {w}val=\"{}\"/>", id_ref(b))));
        }
        if now.next != was.next {
            pending = set_child(&pending, &cx, "next", now.next.as_ref().map(|n| format!("<{w}next {w}val=\"{}\"/>", id_ref(n))));
        }
        if now.quick != was.quick {
            pending = set_child(&pending, &cx, "qFormat", now.quick.then(|| format!("<{w}qFormat/>")));
        }
        if now.auto_update != was.auto_update {
            pending = set_child(&pending, &cx, "autoRedefine", now.auto_update.then(|| format!("<{w}autoRedefine/>")));
        }
        if now.name != was.name {
            pending = set_child(&pending, &cx, "name", Some(format!("<{w}name {w}val=\"{}\"/>", esc(now.name.trim()))));
        }
        if now.kind != was.kind && was.open == StyleOpen::Create {
            let id = parsed(&res.styles, &pending).map(|s| s.id).unwrap_or_default();
            pending =
                with_start(&pending, &format!("<{w}style {w}type=\"{}\" {w}customStyle=\"1\" {w}styleId=\"{}\">", xml_type(now.kind), esc(&id)));
            if now.kind != StyleDialogType::Paragraph && now.kind != StyleDialogType::Linked {
                pending = set_child(&pending, &cx, "next", None);
            }
            if now.kind == StyleDialogType::Character {
                pending = set_child(&pending, &cx, "pPr", None);
            }
        }
        let part = if now.kind == StyleDialogType::Table { now.table_part } else { TableStylePart::WholeTable };
        let mut v = self.style_values(&pending, now.open.clone(), &res.styles, part)?;
        v.short_form = now.short_form;
        v.kind = now.kind;
        // A list style's levels are the dialog's until it applies.
        if now.kind == StyleDialogType::List && !now.list_levels.is_empty() {
            (v.list_levels, v.list_level) = (now.list_levels, now.list_level.min(8));
        }
        Ok(v)
    }

    /// Applies the style dialog: the style added or changed (a new paragraph or linked style applied to the selected
    /// paragraphs, a character style to the selected text), as one undo step.
    pub fn style_dialog_apply(&mut self, v: StyleDialog, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let styles = &res.styles;
        let Some(style) = parsed(styles, &v.pending) else { return Outcome::invalid("not a style") };
        let except = match &v.open {
            StyleOpen::Modify(id) => Some(id.0.as_str()),
            StyleOpen::Create => None,
        };
        let first = v.name.split(',').next().unwrap_or("").trim().to_string();
        if except.and_then(|e| styles.get(e)).is_none_or(|s| !crate::services::styles::describe::display_name(s).eq_ignore_ascii_case(&first)) {
            match check_name(styles, &first, except) {
                Ok(()) => {}
                Err(NameRefusal::Empty) => return Outcome::invalid("a style needs a name"),
                Err(NameRefusal::Duplicate) => return Outcome::invalid("that style name already exists"),
                Err(NameRefusal::Reserved) => return Outcome::invalid("that style name is reserved for a built-in style"),
            }
        }
        if let Some(n) = &style.next
            && !styles.by_id.contains_key(n)
            && *n != style.id
        {
            return Outcome::invalid("the following style doesn't exist");
        }
        let cx = styles.cx();
        let w = cx.w.clone();
        let mut raw = v.pending.clone();
        // A built-in style keeps its name: the new one is an alias.
        if let StyleOpen::Modify(id) = &v.open
            && let Some(s) = styles.get(&id.0)
            && !s.custom
            && !first.eq_ignore_ascii_case(&crate::services::styles::describe::display_name(s))
        {
            let names: Vec<String> = std::iter::once(s.name.clone()).chain(std::iter::once(first.clone())).chain(s.aliases.iter().cloned()).collect();
            raw = set_child(&raw, &cx, "name", Some(format!("<{w}name {w}val=\"{}\"/>", esc(&names.join(",")))));
        }
        let creating = v.open == StyleOpen::Create;
        // A new style takes its id from its name; its following style, if it was itself, follows.
        let id = if creating {
            let id = id_for(&first, styles);
            raw = with_start(&raw, &format!("<{w}style {w}type=\"{}\" {w}customStyle=\"1\" {w}styleId=\"{}\">", xml_type(v.kind), esc(&id)));
            if style.next.as_deref() == Some(style.id.as_str()) {
                raw = set_child(&raw, &cx, "next", Some(format!("<{w}next {w}val=\"{}\"/>", esc(&id))));
            }
            id
        } else {
            style.id.clone()
        };
        let linked = v.kind == StyleDialogType::Linked && creating;
        let character = v.kind == StyleDialogType::Character;
        let kind = v.kind;
        let levels = v.list_levels.clone();
        let label = if creating { "New Style" } else { "Modify Style" };
        let char_id = format!("{id}Char");
        if linked {
            raw = set_child(&raw, &cx, "link", Some(format!("<{w}link {w}val=\"{}\"/>", esc(&char_id))));
        }
        let char_xml = linked.then(|| {
            let rpr = Props::parse(Some(&raw), "style", &cx).get("rPr").map(str::to_string).unwrap_or_default();
            format!(
                "<{w}style {w}type=\"character\" {w}customStyle=\"1\" {w}styleId=\"{c}\"><{w}name {w}val=\"{n} Char\"/><{w}basedOn {w}val=\"DefaultParagraphFont\"/><{w}link {w}val=\"{i}\"/>{rpr}</{w}style>",
                c = esc(&char_id),
                n = esc(&first),
                i = esc(&id)
            )
        });
        self.joined(label, fonts, |e| {
            let out = e.edit_styles(
                label,
                |s| {
                    let mut changed = s.set_xml(raw)?;
                    if let Some(c) = char_xml {
                        changed.extend(s.set_xml(c)?);
                    }
                    Ok(changed)
                },
                fonts,
            );
            if matches!(out, Outcome::Refused(_)) {
                return out;
            }
            // A list style's levels: its own list's definition, made with it.
            if kind == StyleDialogType::List && !levels.is_empty() {
                let defined = e.define_style_list(&id, &levels, fonts);
                if matches!(defined, Outcome::Refused(_)) {
                    return defined;
                }
            }
            if !creating || matches!(out, Outcome::Unchanged(_)) {
                return out;
            }
            match kind {
                StyleDialogType::Table => match e.selection_table() {
                    Some(_) => e.execute(Command::SetTableStyle(StyleId(id.clone())), fonts),
                    None => out,
                },
                StyleDialogType::List => e.apply_list_entry(&crate::api::services::ListEntryId(format!("style:{id}")), fonts),
                _ if character => e.format(&super::format::Format::CharStyle(id.clone()), fonts),
                _ => e.format(&super::format::Format::ParaStyle(id.clone()), fonts),
            }
        })
    }

    /// The style dialog after a Format menu dialog's commands: the style's XML changed by them, and the values read
    /// again.
    pub fn style_dialog_format(&mut self, v: &StyleDialog, commands: Vec<Command>, fonts: &Fonts) -> Result<StyleDialog, Error> {
        self.style_part = if v.kind == StyleDialogType::Table { v.table_part.region() } else { None };
        let formatted = self.format_style(&v.pending, commands, fonts);
        self.style_part = None;
        let pending = formatted?;
        let res = self.doc.resolver.clone().ok_or_else(|| Error::unavailable("no styles"))?;
        let mut out = self.style_values(&pending, v.open.clone(), &res.styles, v.table_part)?;
        (out.short_form, out.kind, out.name) = (v.short_form, v.kind, v.name.clone());
        if v.kind == StyleDialogType::List && !v.list_levels.is_empty() {
            (out.list_levels, out.list_level) = (v.list_levels.clone(), v.list_level);
        }
        Ok(out)
    }

    /// The style dialog with its style's paragraphs in list `num`, or in none.
    pub fn style_dialog_numbering(&mut self, v: &StyleDialog, num: Option<i64>) -> Result<StyleDialog, Error> {
        let res = self.doc.resolver.clone().ok_or_else(|| Error::unavailable("no styles"))?;
        let cx = res.styles.cx();
        let w = cx.w.clone();
        let pending = super::styles::edit_child(&v.pending, "pPr", &cx, |p| match num {
            Some(n) => p.set("numPr", format!("<{w}numPr><{w}numId {w}val=\"{n}\"/></{w}numPr>")),
            None => p.remove("numPr"),
        });
        let mut out = self.style_values(&pending, v.open.clone(), &res.styles, v.table_part)?;
        (out.short_form, out.kind, out.name) = (v.short_form, v.kind, v.name.clone());
        Ok(out)
    }

    /// A table style's region (`w:tblStylePr`), or its whole table.
    fn table_region_of(&self, raw: &str, part: Option<&str>) -> Option<crate::docx::styles::Region> {
        let res = self.doc.resolver.as_ref()?;
        let mut next = (**res).clone();
        next.styles.set_xml(raw.to_string()).ok()?;
        let id = next.styles.by_id.values().find(|s| s.raw == raw)?.id.clone();
        Some(next.styles.table_region(Some(&id), &part.into_iter().collect::<Vec<_>>()))
    }

    /// Table Properties' values for a table style: its alignment, indent, cell margins, row breaking and cell
    /// alignment (what a style holds; the rest unavailable).
    pub fn style_table_properties(&self, raw: &str) -> Option<crate::api::services::TablePropertiesDialog> {
        use crate::api::value::{CellAlignment, HorizontalAlignment, Points, TableWidth};
        let r = self.table_region_of(raw, None)?;
        let m = &r.tbl.margins;
        Some(crate::api::services::TablePropertiesDialog {
            width: match r.tbl.width {
                Some(crate::docx::props::Width::Dxa(v)) => TableWidth::Points { value: Points(v) },
                Some(crate::docx::props::Width::Pct(f)) => TableWidth::Percent { value: f * 100.0 },
                _ => TableWidth::Auto,
            },
            alignment: match r.tbl.jc {
                Some(crate::docx::props::Jc::Center) => HorizontalAlignment::Center,
                Some(crate::docx::props::Jc::Right) => HorizontalAlignment::Right,
                _ => HorizontalAlignment::Left,
            },
            indent: Points(r.tbl.ind.unwrap_or(0.0)),
            row_height: Tri::Unavailable,
            row_can_break: Tri::Uniform(!r.tr.cant_split.unwrap_or(false)),
            header_row: Tri::Unavailable,
            column_width: Tri::Unavailable,
            cell_alignment: Tri::Uniform(match r.tc.v_align.as_deref() {
                Some("center") => CellAlignment::Center,
                Some("bottom") => CellAlignment::Bottom,
                _ => CellAlignment::Top,
            }),
            cell_margins: crate::api::services::CellMarginValues {
                top: Points(m.top.unwrap_or(0.0)),
                bottom: Points(m.bottom.unwrap_or(0.0)),
                left: Points(m.left.unwrap_or(5.4)),
                right: Points(m.right.unwrap_or(5.4)),
            },
        })
    }

    /// Banding's values for a table style.
    pub fn style_banding(&self, raw: &str) -> Option<crate::api::services::BandingDialog> {
        let r = self.table_region_of(raw, None)?;
        Some(crate::api::services::BandingDialog {
            row_band: r.tbl.row_band.unwrap_or(1).max(1) as u32,
            column_band: r.tbl.col_band.unwrap_or(1).max(1) as u32,
        })
    }

    /// The style dialog after a table style's Format entry (Borders and Shading, Table Properties, Banding): what
    /// changed from `start` written into the style, and the values read again.
    pub fn style_dialog_table_format(
        &mut self,
        v: &StyleDialog,
        start: &crate::api::services::DialogValues,
        now: &crate::api::services::DialogValues,
    ) -> Result<StyleDialog, Error> {
        use crate::api::services::DialogValues;
        use crate::docx::write::props::{TBLPR, TCPR, TRPR};
        let res = self.doc.resolver.clone().ok_or_else(|| Error::unavailable("no styles"))?;
        let cx = res.styles.cx();
        let w = cx.w.clone();
        let region = v.table_part.region();
        let root = |raw: &str, child: &str, order: &'static [&'static str], edit: &dyn Fn(&mut Props)| {
            super::styles::edit_child(raw, child, &cx, |p| {
                p.reorder(order);
                edit(p);
            })
        };
        let cell = |raw: &str, edit: &dyn Fn(&mut Props)| match region {
            Some(r) => super::styles::edit_table_part(raw, r, "tcPr", &cx, edit),
            None => root(raw, "tcPr", TCPR, edit),
        };
        let line = |side: &str, b: &Option<crate::api::command::Border>| match b {
            Some(b) => {
                let color = match b.color {
                    AutoOr::Auto => "auto".to_string(),
                    AutoOr::Value(c) => format!("{:06X}", c.0),
                };
                format!(
                    "<{w}{side} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"0\" {w}color=\"{color}\"/>",
                    b.style.val(),
                    (b.width.0 * 8.0).round() as i64
                )
            }
            None => format!("<{w}{side} {w}val=\"nil\"/>"),
        };
        let mut raw = v.pending.clone();
        match (start, now) {
            (DialogValues::BordersShading(a), DialogValues::BordersShading(b)) => {
                let edges = |x: &crate::api::services::BordersShadingDialog| {
                    (x.top.clone(), x.left.clone(), x.bottom.clone(), x.right.clone(), x.between.clone(), x.inside_vertical.clone())
                };
                if edges(a) != edges(b) {
                    let name = if region.is_some() { "tcBorders" } else { "tblBorders" };
                    let xml = format!(
                        "<{w}{name}>{}{}{}{}{}{}</{w}{name}>",
                        line("top", &b.top),
                        line("left", &b.left),
                        line("bottom", &b.bottom),
                        line("right", &b.right),
                        line("insideH", &b.between),
                        line("insideV", &b.inside_vertical)
                    );
                    raw = match region {
                        Some(r) => super::styles::edit_table_part(&raw, r, "tcPr", &cx, |p| p.set("tcBorders", xml.clone())),
                        None => root(&raw, "tblPr", TBLPR, &|p| p.set("tblBorders", xml.clone())),
                    };
                }
                if (a.fill, a.pattern, a.pattern_color) != (b.fill, b.pattern, b.pattern_color) {
                    let hex = |c: Option<Color>| c.map_or("auto".to_string(), |c| format!("{:06X}", c.0));
                    let fill = match b.fill {
                        NoneOr::Value(c) => Some(c),
                        NoneOr::None => None,
                    };
                    let color = match b.pattern_color {
                        AutoOr::Value(c) => Some(c),
                        AutoOr::Auto => None,
                    };
                    let clear = fill.is_none() && b.pattern == crate::api::command::ShadingPattern::Clear;
                    let shd = format!("<{w}shd {w}val=\"{}\" {w}color=\"{}\" {w}fill=\"{}\"/>", b.pattern.val(), hex(color), hex(fill));
                    raw = cell(&raw, &|p| if clear { p.remove("shd") } else { p.set("shd", shd.clone()) });
                }
            }
            (DialogValues::TableProperties(a), DialogValues::TableProperties(b)) => {
                if a.alignment != b.alignment || a.indent != b.indent || a.cell_margins != b.cell_margins {
                    let jc = match b.alignment {
                        crate::api::value::HorizontalAlignment::Left => "left",
                        crate::api::value::HorizontalAlignment::Center => "center",
                        crate::api::value::HorizontalAlignment::Right => "right",
                    };
                    let tw = |p: crate::api::value::Points| (p.0 * 20.0).round() as i64;
                    let m = &b.cell_margins;
                    raw = root(&raw, "tblPr", TBLPR, &|p| {
                        p.set("jc", format!("<{w}jc {w}val=\"{jc}\"/>"));
                        p.set("tblInd", format!("<{w}tblInd {w}w=\"{}\" {w}type=\"dxa\"/>", tw(b.indent)));
                        p.set(
                            "tblCellMar",
                            format!(
                                "<{w}tblCellMar><{w}top {w}w=\"{}\" {w}type=\"dxa\"/><{w}left {w}w=\"{}\" {w}type=\"dxa\"/><{w}bottom {w}w=\"{}\" {w}type=\"dxa\"/><{w}right {w}w=\"{}\" {w}type=\"dxa\"/></{w}tblCellMar>",
                                tw(m.top),
                                tw(m.left),
                                tw(m.bottom),
                                tw(m.right)
                            ),
                        );
                    });
                }
                if let (Tri::Uniform(x), Tri::Uniform(y)) = (&a.row_can_break, &b.row_can_break)
                    && x != y
                {
                    let breaks = *y;
                    raw = root(&raw, "trPr", TRPR, &|p| if breaks { p.remove("cantSplit") } else { p.set("cantSplit", format!("<{w}cantSplit/>")) });
                }
                if let (Tri::Uniform(x), Tri::Uniform(y)) = (&a.cell_alignment, &b.cell_alignment)
                    && x != y
                {
                    let va = match y {
                        crate::api::value::CellAlignment::Top => "top",
                        crate::api::value::CellAlignment::Center => "center",
                        crate::api::value::CellAlignment::Bottom => "bottom",
                    };
                    raw = cell(&raw, &|p| p.set("vAlign", format!("<{w}vAlign {w}val=\"{va}\"/>")));
                }
            }
            (DialogValues::Banding(a), DialogValues::Banding(b)) if a != b => {
                let (rows, cols) = (b.row_band.max(1), b.column_band.max(1));
                raw = root(&raw, "tblPr", TBLPR, &|p| {
                    p.set("tblStyleRowBandSize", format!("<{w}tblStyleRowBandSize {w}val=\"{rows}\"/>"));
                    p.set("tblStyleColBandSize", format!("<{w}tblStyleColBandSize {w}val=\"{cols}\"/>"));
                });
            }
            _ => {}
        }
        let mut out = self.style_values(&raw, v.open.clone(), &res.styles, v.table_part)?;
        (out.short_form, out.kind, out.name) = (v.short_form, v.kind, v.name.clone());
        Ok(out)
    }

    /// The formatting state of text in a style's XML: for the Format menu's dialogs.
    pub fn style_formatting_state(&self, raw: &str) -> Option<(CharacterFormatting, ParagraphFormatting, Vec<crate::api::services::TabStopInfo>)> {
        let (para, run, _) = self.style_formatting(raw)?;
        let tabs =
            para.style.tabs.iter().filter(|t| t.kind != crate::docx::props::TabKind::Clear).map(|t| super::commands::tabs::info(t, false)).collect();
        Some((super::state::character(&[&run]), super::state::paragraph(&[(&para.style, None)]), tabs))
    }
}
