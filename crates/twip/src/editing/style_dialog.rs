//! The style dialog

use std::rc::Rc;

use crate::api::command::{Command, Indents, ParagraphSpacing};
use crate::api::error::Error;
use crate::api::services::{StyleDialog, StyleDialogType, StyleOpen, TableStylePart};
use crate::api::state::{CharacterFormatting, ParagraphFormatting, Tri};
use crate::api::value::{AutoOr, NoneOr, StyleId};
use crate::docx::resolvers::TypedDefs;
use crate::docx::write::props::{Cx, Props, STYLE, esc};
use crate::font::Fonts;
use crate::format::{StyleKind, StyleSheet};
use crate::services::styles::{NameRefusal, check_name, describe, next_new_name};

use super::Editor;
use super::commands::Outcome;
use crate::docx::run_xml;

fn one<T: Clone + Default>(t: &Tri<T>) -> T {
    match t {
        Tri::Uniform(v) => v.clone(),
        _ => T::default(),
    }
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

/// Built-in style `id`'s definition, when the document doesn't define it.
fn built_in_definition(typed: &TypedDefs, id: &str) -> Option<String> {
    if typed.cascade.styles.styles.contains_key(id) {
        return None;
    }
    let b = crate::services::styles::builtin::BUILT_IN.iter().find(|b| b.id == id)?;
    Some(crate::docx::style_xml::serialize(&crate::services::styles::builtin::definition(b), &typed.cascade.colours, &typed.styles.cx()))
}

/// The kind of style a dialog type makes.
fn kind_of(kind: StyleDialogType) -> StyleKind {
    match kind {
        StyleDialogType::Character => StyleKind::Character,
        StyleDialogType::Table => StyleKind::Table,
        StyleDialogType::List => StyleKind::Numbering,
        _ => StyleKind::Paragraph,
    }
}

/// The style a new style of `kind` is based on when its base is of another kind: the default paragraph, character
/// or table style; none for a list style.
fn default_base(styles: &StyleSheet, kind: StyleKind) -> Option<String> {
    let id = match kind {
        StyleKind::Paragraph => styles.default_paragraph.as_deref().map(str::to_string),
        StyleKind::Character => Some("DefaultParagraphFont".to_string()),
        StyleKind::Table => styles.default_table.as_deref().map(str::to_string),
        StyleKind::Numbering => None,
    }?;
    styles.get(&id).is_some_and(|s| s.kind == kind).then_some(id)
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
        let v = super::table_format::v_align_xml(&w, super::table_format::cell_alignment(now.cell_alignment));
        raw = cell(&raw, &|p| p.set("vAlign", v.clone()));
    }
    if now.border != was.border {
        let edges = |name: &str| -> String {
            let one = |side: &str| match &now.border {
                Some(b) => {
                    let color = match super::readback::rgb_only(&b.color) {
                        AutoOr::Auto => "auto".to_string(),
                        AutoOr::Value(c) => format!("{:06X}", c.0),
                    };
                    format!(
                        "<{w}{side} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"0\" {w}color=\"{color}\"/>",
                        crate::docx::line_style::border_val(super::readback::border_style(b.style)),
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
fn id_for(name: &str, styles: &StyleSheet) -> String {
    let base: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
    let base = if base.is_empty() { "Style".to_string() } else { base };
    (0..).map(|n| if n == 0 { base.clone() } else { format!("{base}{n}") }).find(|id| !styles.styles.contains_key(id.as_str())).unwrap_or(base)
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
        let typed = res.typed();
        let styles = &*typed.cascade.styles;
        let cx = typed.styles.cx();
        let w = &cx.w;
        let pending = match &open {
            StyleOpen::Modify(id) => match typed.style_xml(&id.0) {
                Some(x) => x.into_owned(),
                None => built_in_definition(&typed, &id.0).ok_or_else(|| Error::unavailable("no such style"))?,
            },
            StyleOpen::Create => {
                let name = next_new_name(styles);
                let para = self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para(&self.doc, p));
                let base = para.and_then(|p| p.style.style_id.clone()).or_else(|| styles.default_paragraph.as_deref().map(str::to_string));
                let (ppr, rpr) = self.selection_formatting();
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
        let mut v = self.style_values(&pending, open, TableStylePart::WholeTable)?;
        v.short_form = short_form;
        Ok(v)
    }

    /// The dialog's values for a style's XML: its name, type and flags, its formatting as text in it resolves, and
    /// its description.
    fn style_values(&self, pending: &str, open: StyleOpen, part: TableStylePart) -> Result<StyleDialog, Error> {
        use crate::docx::table_props::{tbl_props, tc_props};
        let res = self.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?;
        let (next, id) = res.with_style_xml(pending).ok_or_else(|| Error::invalid("not a style"))?;
        let cascade = next.cascade();
        let entry = cascade.styles.styles.get(&id).ok_or_else(|| Error::invalid("not a style"))?;
        let style = &*entry.style;
        let colours = &cascade.colours;
        // A table style shows its part's formatting: the whole table's, with the part's over it.
        let typed =
            (style.kind == StyleKind::Table).then(|| cascade.table_region(Some(&style.id), &crate::docx::style_xml::region_kinds(part.region())));
        let region = typed.as_ref().map(|t| tc_props(&t.format.cell, colours));
        let (c, p): (CharacterFormatting, ParagraphFormatting) = match &typed {
            Some(typed) => {
                let para = crate::docx::para_xml::resolve(&next, &Default::default(), None, Some(typed));
                let run = Rc::new(run_xml::resolve(&next, &para.base, Default::default(), false));
                (super::state::character(&[&run]), super::state::paragraph(&[(&para.style, None)]))
            }
            None => {
                let (para, run, _) = self.style_formatting(pending).ok_or_else(|| Error::invalid("not a style"))?;
                (super::state::character(&[&run]), super::state::paragraph(&[(&para.style, None)]))
            }
        };
        let whole = (style.kind == StyleKind::Table).then(|| tbl_props(&cascade.table_region(Some(&style.id), &[]).format.table, colours));
        let line = |b: &Option<crate::docx::props::Border>| super::borders_dialog::line(b);
        let border = match (&region, part) {
            (Some(_), TableStylePart::WholeTable) => whole.as_ref().and_then(|t| line(&t.borders.top)),
            (Some(tc), _) => line(&tc.borders.top),
            _ => None,
        };
        let list_levels = match style.kind {
            StyleKind::Numbering => {
                let num = match cascade.chain_para(Some(&style.id)).list {
                    Some(crate::format::ListChoice::In(n)) if n.0 != 0 => Some(n.0),
                    _ => None,
                };
                num.and_then(|n| self.list_levels(n)).unwrap_or_else(|| super::list_defs::complete(&[]))
            }
            _ => Vec::new(),
        };
        let points = |t: &Tri<AutoOr<crate::api::value::Points>>| match t {
            Tri::Uniform(AutoOr::Value(v)) => *v,
            _ => crate::api::value::Points(0.0),
        };
        let on = |v: Option<bool>| v.unwrap_or(false);
        Ok(StyleDialog {
            open,
            short_form: false,
            name: std::iter::once(crate::services::styles::describe::display_name(style))
                .chain(style.aliases.iter().map(|a| a.to_string()))
                .collect::<Vec<_>>()
                .join(","),
            kind: match style.kind {
                StyleKind::Paragraph if style.link.is_some() => StyleDialogType::Linked,
                StyleKind::Paragraph => StyleDialogType::Paragraph,
                StyleKind::Character => StyleDialogType::Character,
                StyleKind::Table => StyleDialogType::Table,
                StyleKind::Numbering => StyleDialogType::List,
            },
            based_on: cascade.based_on(entry).map(|b| StyleId(b.to_string())),
            next: style.next.as_deref().map(|n| StyleId(n.to_string())),
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
            quick: on(style.quick),
            auto_update: on(style.meta.auto_redefine),
            table_part: part,
            fill: region.as_ref().and_then(|tc| tc.shading.flatten()).map(crate::api::value::Color),
            cell_alignment: super::table_format::api_cell_alignment(region.as_ref().and_then(|tc| tc.v_align)),
            border,
            row_band: whole.as_ref().and_then(|t| t.row_band).unwrap_or(1).max(1) as u32,
            column_band: whole.as_ref().and_then(|t| t.col_band).unwrap_or(1).max(1) as u32,
            list_levels,
            list_level: 0,
            description: describe::describe(entry, &cascade),
            pending: pending.to_string(),
        })
    }

    /// The style dialog after the user changed `now` from `was`: the formatting it shows set on the style, the
    /// name, base, following style and flags written into it, and the values read again.
    pub fn style_dialog_update(&mut self, was: &StyleDialog, now: StyleDialog, fonts: &Fonts) -> Result<StyleDialog, Error> {
        let res = self.doc.resolver.clone().ok_or_else(|| Error::unavailable("no styles"))?;
        let typed = res.typed();
        let cx = typed.styles.cx();
        let w = cx.w.clone();
        if now.border != was.border {
            super::commands::bounds::border(now.border.as_ref())?;
        }
        let para = matches!(now.kind, StyleDialogType::Paragraph | StyleDialogType::Linked);
        let table = now.kind == StyleDialogType::Table;
        let mut commands = Vec::new();
        if now.font != was.font && !now.font.trim().is_empty() {
            commands.push(Command::SetFont(crate::api::command::FontChoice::Name(now.font.clone())));
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
            commands.push(Command::SetFontColor(super::commands::rgb_value(now.color)));
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
                commands.push(Command::SetParagraphSpacing(ParagraphSpacing { before, after, contextual: None }));
            }
            if now.left_indent != was.left_indent {
                commands.push(Command::SetIndents(Indents {
                    left: Some(now.left_indent),
                    right: None,
                    first_line: None,
                    hanging: None,
                    mirror: None,
                }));
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
            let id = typed.styles.style_from_xml(&pending).map(|s| s.id.to_string()).unwrap_or_default();
            pending =
                with_start(&pending, &format!("<{w}style {w}type=\"{}\" {w}customStyle=\"1\" {w}styleId=\"{}\">", xml_type(now.kind), esc(&id)));
            if now.kind != StyleDialogType::Paragraph && now.kind != StyleDialogType::Linked {
                pending = set_child(&pending, &cx, "next", None);
            }
            if now.kind == StyleDialogType::Character {
                pending = set_child(&pending, &cx, "pPr", None);
            }
            let kind = kind_of(now.kind);
            let sheet = &*typed.cascade.styles;
            let base_of = res.with_style_xml(&pending).and_then(|(n, id)| {
                let c = n.cascade();
                c.styles.styles.get(&id).and_then(|e| c.based_on(e).map(Rc::<str>::from))
            });
            if base_of.is_some_and(|b| sheet.get(&b).is_none_or(|b| b.kind != kind)) {
                let base = default_base(sheet, kind).map(|b| format!("<{w}basedOn {w}val=\"{}\"/>", esc(&b)));
                pending = set_child(&pending, &cx, "basedOn", base);
            }
        }
        let part = if now.kind == StyleDialogType::Table { now.table_part } else { TableStylePart::WholeTable };
        let mut v = self.style_values(&pending, now.open.clone(), part)?;
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
        let Some(mut res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        if let StyleOpen::Modify(id) = &v.open
            && let Some(def) = built_in_definition(&res.typed(), &id.0)
        {
            let _ = std::rc::Rc::make_mut(&mut res).set_style_xml(def);
        }
        let typed = res.typed();
        let c = &*typed.cascade;
        let styles = &*c.styles;
        let xml = |id: &str| typed.style_xml(id).map(std::borrow::Cow::into_owned);
        let Some((next, pending_id)) = res.with_style_xml(&v.pending) else { return Outcome::invalid("not a style") };
        let pending = next.cascade();
        let Some(entry) = pending.styles.styles.get(&pending_id) else { return Outcome::invalid("not a style") };
        let style = &*entry.style;
        let style_base = pending.based_on(entry);
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
        // The style the dialog opened on, as it was
        if let StyleOpen::Modify(id) = &v.open
            && styles.get(&id.0).is_none_or(|s| *s.id != *style.id || s.kind != style.kind || s.kind != kind_of(v.kind))
        {
            return Outcome::invalid("the style's id and type are the ones the dialog opened on");
        }
        // A base the dialog set: an existing style of the same type
        let open = except.and_then(|e| styles.styles.get(e));
        let kind = open.map_or(kind_of(v.kind), |o| o.style.kind);
        if let Some(b) = style_base
            && open.is_none_or(|o| c.based_on(o) != Some(b))
            && styles.get(b).is_none_or(|b| b.kind != kind)
        {
            return Outcome::invalid("the base style doesn't exist or is of another type");
        }
        if let Some(n) = &style.next
            && *n != style.id
            && styles.get(n).is_none_or(|n| n.kind != StyleKind::Paragraph)
        {
            return Outcome::invalid("the following style doesn't exist");
        }
        let cx = typed.styles.cx();
        let w = cx.w.clone();
        let mut raw = v.pending.clone();
        // A built-in style keeps its name: the new one is an alias.
        if let StyleOpen::Modify(id) = &v.open
            && let Some(s) = styles.get(&id.0)
            && !s.custom.unwrap_or(false)
            && !first.eq_ignore_ascii_case(&crate::services::styles::describe::display_name(s))
        {
            let names: Vec<String> = std::iter::once(s.name.as_deref().unwrap_or_default().to_string())
                .chain(std::iter::once(first.clone()))
                .chain(s.aliases.iter().map(|a| a.to_string()))
                .collect();
            raw = set_child(&raw, &cx, "name", Some(format!("<{w}name {w}val=\"{}\"/>", esc(&names.join(",")))));
        }
        let creating = v.open == StyleOpen::Create;
        // A new style takes its id from its name; its following style, if it was itself, follows.
        let id = if creating {
            let id = id_for(&first, styles);
            raw = with_start(&raw, &format!("<{w}style {w}type=\"{}\" {w}customStyle=\"1\" {w}styleId=\"{}\">", xml_type(v.kind), esc(&id)));
            if style.next.as_deref() == Some(&*style.id) {
                raw = set_child(&raw, &cx, "next", Some(format!("<{w}next {w}val=\"{}\"/>", esc(&id))));
            }
            id
        } else {
            style.id.to_string()
        };
        let linked = v.kind == StyleDialogType::Linked && creating;
        let character = v.kind == StyleDialogType::Character;
        let kind = v.kind;
        let levels = v.list_levels.clone();
        let label = if creating { "New Style" } else { "Modify Style" };
        let char_id = (0..)
            .map(|n| if n == 0 { format!("{id}Char") } else { format!("{id}Char{n}") })
            .find(|c| !styles.styles.contains_key(c.as_str()))
            .unwrap_or_default();
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
        // A linked style's character half takes its character formatting.
        let char_xml = char_xml.or_else(|| {
            let half = style
                .link
                .as_deref()
                .filter(|l| styles.get(l).is_some_and(|c| c.kind == StyleKind::Character && c.link.as_deref() == Some(id.as_str())))?;
            let rpr = Props::parse(Some(&raw), "style", &cx).get("rPr").map(str::to_string);
            let was = xml(&id).and_then(|x| Props::parse(Some(&x), "style", &cx).get("rPr").map(str::to_string));
            (!creating && style.kind == StyleKind::Paragraph && rpr != was).then(|| set_child(&xml(half).unwrap_or_default(), &cx, "rPr", rpr))
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
        self.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?;
        let mut out = self.style_values(&pending, v.open.clone(), v.table_part)?;
        (out.short_form, out.kind, out.name) = (v.short_form, v.kind, v.name.clone());
        if v.kind == StyleDialogType::List && !v.list_levels.is_empty() {
            (out.list_levels, out.list_level) = (v.list_levels.clone(), v.list_level);
        }
        Ok(out)
    }

    /// The style dialog with its style's paragraphs in list `num`, or in none.
    pub fn style_dialog_numbering(&mut self, v: &StyleDialog, num: Option<i64>) -> Result<StyleDialog, Error> {
        let res = self.doc.resolver.clone().ok_or_else(|| Error::unavailable("no styles"))?;
        let cx = res.typed().styles.cx();
        let w = cx.w.clone();
        let pending = super::styles::edit_child(&v.pending, "pPr", &cx, |p| match num {
            Some(n) => p.set("numPr", format!("<{w}numPr><{w}numId {w}val=\"{n}\"/></{w}numPr>")),
            None => p.remove("numPr"),
        });
        let mut out = self.style_values(&pending, v.open.clone(), v.table_part)?;
        (out.short_form, out.kind, out.name) = (v.short_form, v.kind, v.name.clone());
        Ok(out)
    }

    /// A table style's table, cell and row properties: its region (`w:tblStylePr`)'s, or its whole table's.
    fn table_region_of(
        &self,
        raw: &str,
        part: Option<&str>,
    ) -> Option<(crate::docx::props::TblProps, crate::docx::props::TcProps, crate::docx::props::TrProps)> {
        use crate::docx::table_props::{tbl_props, tc_props, tr_props};
        let (next, id) = self.doc.resolver.as_ref()?.with_style_xml(raw)?;
        let c = next.cascade();
        let f = c.table_region(Some(&id), &crate::docx::style_xml::region_kinds(part)).format;
        Some((tbl_props(&f.table, &c.colours), tc_props(&f.cell, &c.colours), tr_props(&f.row)))
    }

    /// Table Properties' values for a table style: its alignment, indent, cell margins, row breaking and cell
    /// alignment (what a style holds; the rest unavailable).
    pub fn style_table_properties(&self, raw: &str) -> Option<crate::api::services::TablePropertiesDialog> {
        use crate::api::value::{HorizontalAlignment, Points, TableWidth};
        let (tbl, tc, tr) = self.table_region_of(raw, None)?;
        let m = &tbl.margins;
        Some(crate::api::services::TablePropertiesDialog {
            width: match tbl.width {
                Some(crate::docx::props::Width::Dxa(v)) => TableWidth::Points { value: Points(v) },
                Some(crate::docx::props::Width::Pct(f)) => TableWidth::Percent { value: f * 100.0 },
                _ => TableWidth::Auto,
            },
            alignment: match tbl.jc {
                Some(crate::docx::props::Jc::Center) => HorizontalAlignment::Center,
                Some(crate::docx::props::Jc::Right) => HorizontalAlignment::Right,
                _ => HorizontalAlignment::Left,
            },
            indent: Points(tbl.ind.map_or(0.0, crate::docx::props::Width::dxa)),
            row_height: Tri::Unavailable,
            row_can_break: Tri::Uniform(!tr.cant_split.unwrap_or(false)),
            header_row: Tri::Unavailable,
            column_width: Tri::Unavailable,
            cell_alignment: Tri::Uniform(super::table_format::api_cell_alignment(tc.v_align)),
            cell_margins: crate::api::services::CellMarginValues {
                top: Points(m.top.unwrap_or(0.0)),
                bottom: Points(m.bottom.unwrap_or(0.0)),
                left: Points(m.left.unwrap_or(5.4)),
                right: Points(m.right.unwrap_or(5.4)),
            },
            ..Default::default()
        })
    }

    /// Banding's values for a table style.
    pub fn style_banding(&self, raw: &str) -> Option<crate::api::services::BandingDialog> {
        let (tbl, _, _) = self.table_region_of(raw, None)?;
        Some(crate::api::services::BandingDialog {
            row_band: tbl.row_band.unwrap_or(1).max(1) as u32,
            column_band: tbl.col_band.unwrap_or(1).max(1) as u32,
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
        use crate::docx::write::props::{TBLPR, TC_BORDERS, TCPR, TRPR};
        let res = self.doc.resolver.clone().ok_or_else(|| Error::unavailable("no styles"))?;
        let cx = res.typed().styles.cx();
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
                let color = match super::readback::rgb_only(&b.color) {
                    AutoOr::Auto => "auto".to_string(),
                    AutoOr::Value(c) => format!("{:06X}", c.0),
                };
                format!(
                    "<{w}{side} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"0\" {w}color=\"{color}\"/>",
                    crate::docx::line_style::border_val(super::readback::border_style(b.style)),
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
                let edged = edges(a) != edges(b);
                // The lines written: every edge when one changed, a diagonal when it did
                let written = [&b.top, &b.bottom, &b.left, &b.right, &b.between, &b.inside_vertical].map(|e| (e, edged));
                let crossed = [(&b.diagonal_down, a.diagonal_down != b.diagonal_down), (&b.diagonal_up, a.diagonal_up != b.diagonal_up)];
                for (e, _) in written.into_iter().chain(crossed).filter(|x| x.1) {
                    super::commands::bounds::border(e.as_ref())?;
                }
                // A diagonal as set when it changed (nil once turned off), else as the style has it.
                let cross = |side: &str, was: &Option<crate::api::command::Border>, now: &Option<crate::api::command::Border>| {
                    (was != now).then(|| line(side, now))
                };
                let (down, up) = (cross("tl2br", &a.diagonal_down, &b.diagonal_down), cross("tr2bl", &a.diagonal_up, &b.diagonal_up));
                let sides = [
                    ("top", &b.top),
                    ("left", &b.left),
                    ("bottom", &b.bottom),
                    ("right", &b.right),
                    ("insideH", &b.between),
                    ("insideV", &b.inside_vertical),
                ];
                if edged && region.is_none() {
                    let xml = format!("<{w}tblBorders>{}</{w}tblBorders>", sides.iter().map(|(side, x)| line(side, x)).collect::<String>());
                    raw = root(&raw, "tblPr", TBLPR, &|p| p.set("tblBorders", xml.clone()));
                }
                // A part's cells take the diagonals with their edges; the whole table's go in its cells' properties.
                if (edged && region.is_some()) || down.is_some() || up.is_some() {
                    raw = cell(&raw, &|p| {
                        let mut bd = Props::parse(p.get("tcBorders"), "tcBorders", &cx);
                        bd.reorder(TC_BORDERS);
                        if edged && region.is_some() {
                            for side in ["top", "start", "left", "bottom", "end", "right", "insideH", "insideV"] {
                                bd.remove(side);
                            }
                            for (side, x) in &sides {
                                bd.set(side, line(side, x));
                            }
                        }
                        for (side, x) in [("tl2br", &down), ("tr2bl", &up)] {
                            if let Some(x) = x {
                                bd.set(side, x.clone());
                            }
                        }
                        match bd.xml() {
                            Some(x) => p.set("tcBorders", x),
                            None => p.remove("tcBorders"),
                        }
                    });
                }
                if (a.fill, a.pattern, a.pattern_color) != (b.fill, b.pattern, b.pattern_color) {
                    let fill = match b.fill {
                        NoneOr::Value(c) => Some(c),
                        NoneOr::None => None,
                    };
                    let color = match b.pattern_color {
                        AutoOr::Value(c) => Some(c),
                        AutoOr::Auto => None,
                    };
                    let clear = fill.is_none() && b.pattern == crate::api::command::ShadingPattern::Clear;
                    let sh = super::table_format::Shading {
                        pattern: b.pattern.val(),
                        color: color.map(|c| super::theme::WordColour::rgb(c.0)),
                        fill: fill.map(|c| super::theme::WordColour::rgb(c.0)),
                    };
                    raw = cell(&raw, &|p| {
                        if clear {
                            p.remove("shd");
                        } else {
                            let x = sh.xml(&w, p.get("shd"), None);
                            p.set("shd", x);
                        }
                    });
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
                    use super::commands::bounds::{MAX, within};
                    within("the indent", b.indent.0, -MAX, MAX)?;
                    for v in [m.top, m.left, m.bottom, m.right] {
                        within("the cell margin", v.0, 0.0, MAX)?;
                    }
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
                    let va = super::table_format::v_align_xml(&w, super::table_format::cell_alignment(*y));
                    raw = cell(&raw, &|p| p.set("vAlign", va.clone()));
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
        let mut out = self.style_values(&raw, v.open.clone(), v.table_part)?;
        (out.short_form, out.kind, out.name) = (v.short_form, v.kind, v.name.clone());
        Ok(out)
    }
}
