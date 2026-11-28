//! The style dialog

use crate::docx::definitions::DefinitionsXml;
use crate::docx::style_xml::StylesPart;
use std::rc::Rc;

use crate::api::command::{Command, Indents, ParagraphSpacing};
use crate::api::error::Error;
use crate::api::services::{StyleDialog, StyleDialogType, StyleOpen, TableStylePart};
use crate::api::state::{CharacterFormatting, ParagraphFormatting, Tri};
use crate::api::value::{AutoOr, NoneOr, StyleId};
use crate::document::format::{Jc, ShadingPattern, Width};
use crate::docx::definitions::TypedDefs;
use crate::docx::style_xml::StyleDef;
use crate::docx::style_xml::StyleXml;
use crate::font::Fonts;
use crate::format::{
    CellMargins, CellProp, Color, ListChoice, ParaProp, RegionKind, RowProp, Shading, Style, StyleKind, StyleProp, StyleSet, StyleSheet,
    TableBorders, TableProp, TableSide, TextBorder,
};
use crate::services::styles::{NameRefusal, check_name, describe, next_new_name};

use super::Editor;
use super::commands::Outcome;
use super::style_defs::{char_half, created_style};
use super::styles::{edit_cells, edit_paras, edit_rows, edit_style, edit_tables, set_flag, set_names, style_holder};
use crate::docx::run_xml;

fn one<T: Clone + Default>(t: &Tri<T>) -> T {
    match t {
        Tri::Uniform(v) => v.clone(),
        _ => T::default(),
    }
}

/// Built-in style `id`'s definition, when the document doesn't define it.
fn built_in_definition(typed: &TypedDefs, id: &str) -> Option<StyleDef> {
    if typed.cascade.styles.styles.contains_key(id) {
        return None;
    }
    let b = crate::services::styles::builtin::BUILT_IN.iter().find(|b| b.id == id)?;
    Some(StyleDef::typed(Rc::new(crate::services::styles::builtin::definition(b))))
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

/// `s` as a new style of the dialog's type `kind` with id `id`, not built in: its own attributes those alone.
pub(super) fn made_as(s: &mut Style, set: &mut Vec<StyleSet>, kind: StyleDialogType, id: &str) {
    (s.kind, s.custom, s.default, s.id) = (kind_of(kind), Some(true), None, id.into());
    if let Some(f) = s.foreign.as_mut() {
        let f = Rc::make_mut(f);
        f.attributes.retain(|a| a.on.is_some());
        if *f == crate::format::ForeignProps::default() {
            s.foreign = None;
        }
    }
    set.extend([StyleProp::Kind, StyleProp::Custom, StyleProp::Default, StyleProp::Id].map(StyleSet::Style));
}

/// A border the dialog writes: its line, width in whole eighths of a point and colour (RGB or automatic), nothing
/// between it and the text; `None` as no line.
fn dialog_border(b: &Option<crate::api::command::Border>) -> TextBorder {
    match b {
        Some(b) => TextBorder {
            style: super::readback::border_style(b.style),
            width: (b.width.0 * 8.0).round() / 8.0,
            space: 0.0,
            shadow: false,
            color: Some(match super::readback::rgb_only(&b.color) {
                AutoOr::Auto => Color::Auto,
                AutoOr::Value(c) => Color::Rgb(c.0),
            }),
        },
        None => TextBorder { style: crate::docx::props::BorderStyle::Nil, width: 0.0, space: 0.0, shadow: false, color: None },
    }
}

/// Every side the dialog's border sets: the outside edges and the inside ones.
const SIDES: [TableSide; 6] = [TableSide::Top, TableSide::Left, TableSide::Bottom, TableSide::Right, TableSide::InsideH, TableSide::InsideV];

/// The outside and inside borders `b` gives, the diagonals none.
fn borders_of(b: [TextBorder; 6]) -> TableBorders {
    let [top, left, bottom, right, inside_h, inside_v] = b.map(Some);
    TableBorders { top, left, bottom, right, inside_h, inside_v, ..TableBorders::default() }
}

/// A table style with the dialog's fill, cell alignment, border and band sizes set where they changed: for the part
/// `region`, or the whole table.
pub(super) fn table_style_edits(def: &StyleDef, was: &StyleDialog, now: &StyleDialog, region: Option<RegionKind>) -> StyleDef {
    edit_style(def, |s, set| {
        if now.fill != was.fill {
            let shading = now.fill.map(|c| Shading { pattern: ShadingPattern::Clear, color: Some(Color::Auto), fill: Some(Color::Rgb(c.0)) });
            edit_cells(s, set, region, |e| {
                e.shading = shading;
                e.whole(CellProp::Shading);
            });
        }
        if now.cell_alignment != was.cell_alignment {
            edit_cells(s, set, region, |e| {
                e.v_align = Some(super::table_format::cell_alignment(now.cell_alignment));
                e.whole(CellProp::VAlign);
            });
        }
        if now.border != was.border {
            let borders = borders_of([(); 6].map(|_| dialog_border(&now.border)));
            match region {
                Some(_) => edit_cells(s, set, region, |e| {
                    e.borders = borders;
                    e.whole(CellProp::Borders);
                }),
                None => edit_tables(s, set, None, |e| {
                    e.borders = borders;
                    e.whole(TableProp::Borders);
                }),
            }
        }
        if now.row_band != was.row_band || now.column_band != was.column_band {
            let (rows, cols) = (now.row_band.max(1), now.column_band.max(1));
            edit_tables(s, set, None, |e| {
                (e.row_band, e.col_band) = (Some(i64::from(rows)), Some(i64::from(cols)));
                e.whole(TableProp::RowBand);
                e.whole(TableProp::ColBand);
            });
        }
    })
}

/// A style with the dialog's base, following style, flags and name set where they changed from `was` to `now`.
pub(super) fn dialog_fields(def: &StyleDef, was: &StyleDialog, now: &StyleDialog) -> StyleDef {
    edit_style(def, |s, set| {
        if now.based_on != was.based_on {
            s.based_on = now.based_on.as_ref().map(|b| Rc::from(b.0.as_str()));
            set.push(StyleSet::Style(StyleProp::BasedOn));
        }
        if now.next != was.next {
            s.next = now.next.as_ref().map(|n| Rc::from(n.0.as_str()));
            set.push(StyleSet::Style(StyleProp::Next));
        }
        if now.quick != was.quick {
            set_flag(s, set, StyleProp::Quick, now.quick);
        }
        if now.auto_update != was.auto_update {
            set_flag(s, set, StyleProp::AutoRedefine, now.auto_update);
        }
        if now.name != was.name {
            set_names(s, set, now.name.trim());
        }
    })
}

/// A new style made a style of the dialog's type `kind`: only a paragraph style follows with another, and a
/// character style has no paragraph formatting.
pub(super) fn retyped(def: &StyleDef, kind: StyleDialogType) -> StyleDef {
    let id = def.get().id.clone();
    edit_style(def, |s, set| {
        made_as(s, set, kind, &id);
        if kind != StyleDialogType::Paragraph && kind != StyleDialogType::Linked {
            s.next = None;
            set.push(StyleSet::Style(StyleProp::Next));
        }
        if kind == StyleDialogType::Character {
            s.format.para = Default::default();
        }
    })
}

/// A style with its paragraphs in list `num`, or in none.
pub(super) fn numbered(def: &StyleDef, num: Option<i64>) -> StyleDef {
    edit_style(def, |s, set| {
        edit_paras(s, set, None, |e| {
            (e.list, e.list_level) = (num.map(|n| ListChoice::In(crate::document::paragraph::ListId(n))), None);
            e.whole(ParaProp::List);
        })
    })
}

/// A style id from a name: its letters and digits.
fn id_for(name: &str, styles: &StyleSheet) -> String {
    let base: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
    let base = if base.is_empty() { "Style".to_string() } else { base };
    (0..).map(|n| if n == 0 { base.clone() } else { format!("{base}{n}") }).find(|id| !styles.styles.contains_key(id.as_str())).unwrap_or(base)
}

/// The region a dialog's table style part is.
fn region(part: TableStylePart) -> Option<RegionKind> {
    part.region().and_then(|r| crate::docx::style_xml::region_kind(Some(r)))
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

    /// The style the dialog has as changed so far.
    fn pending(&self, v: &StyleDialog) -> Result<StyleDef, Error> {
        self.style_of_xml(&v.pending).ok_or_else(|| Error::invalid("not a style"))
    }

    /// Opens the style dialog: on a style, or on a new style from the selection's formatting.
    pub fn style_dialog_open(&self, open: StyleOpen) -> Result<StyleDialog, Error> {
        let res = self.doc.definitions.as_ref().ok_or_else(|| Error::unavailable("no styles"))?;
        let typed = res.typed();
        let styles = &*typed.cascade.styles;
        let pending = match &open {
            StyleOpen::Modify(id) => match style_holder(&typed, &id.0) {
                Some(d) => d,
                None => built_in_definition(&typed, &id.0).ok_or_else(|| Error::unavailable("no such style"))?,
            },
            StyleOpen::Create => {
                let name = next_new_name(styles);
                let para = self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para(&self.doc, p));
                let base = para.and_then(|p| p.style.style_id.clone()).or_else(|| styles.default_paragraph.as_deref().map(str::to_string));
                let (ppr, rpr) = self.selection_formatting();
                let s = created_style(&id_for(&name, styles), StyleKind::Paragraph, &name, base.as_deref(), ppr, rpr);
                StyleDef::typed(Rc::new(s))
            }
        };
        let short_form = open == StyleOpen::Create;
        let mut v = self.style_values(&pending, open, TableStylePart::WholeTable)?;
        v.short_form = short_form;
        Ok(v)
    }

    /// The dialog's values for a style: its name, type and flags, its formatting as text in it resolves, and its
    /// description.
    fn style_values(&self, pending: &StyleDef, open: StyleOpen, part: TableStylePart) -> Result<StyleDialog, Error> {
        use crate::docx::table_props::{tbl_props, tc_props};
        let res = self.doc.definitions.as_ref().ok_or_else(|| Error::unavailable("no styles"))?;
        let mut next = (**res).clone();
        next.set_style(pending.clone()).map_err(|_| Error::invalid("not a style"))?;
        let id = pending.get().id.clone();
        let cascade = next.cascade();
        let entry = cascade.styles.styles.get(&id).ok_or_else(|| Error::invalid("not a style"))?;
        let style = &*entry.style;
        let colours = &cascade.colours;
        // A table style shows its part's formatting: the whole table's, with the part's over it.
        let typed = (style.kind == StyleKind::Table).then(|| cascade.table_region(Some(&style.id), &region(part).into_iter().collect::<Vec<_>>()));
        let region = typed.as_ref().map(|t| tc_props(&t.format.cell, colours));
        let (c, p): (CharacterFormatting, ParagraphFormatting) = match &typed {
            Some(typed) => {
                let para = crate::docx::para_xml::resolve(&next, &Default::default(), None, Some(typed));
                let run = Rc::new(run_xml::resolve(&next, &para.base, Default::default(), false));
                (super::state::character(&[&run]), super::state::paragraph(&[(&para.style, None)]))
            }
            None => {
                let (para, run, _) = self.style_text(pending).ok_or_else(|| Error::invalid("not a style"))?;
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
                    Some(ListChoice::In(n)) if n.0 != 0 => Some(n.0),
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
            pending: self.style_xml_of(pending),
        })
    }

    /// The style dialog after the user changed `now` from `was`: the formatting it shows set on the style, the
    /// name, base, following style and flags written into it, and the values read again.
    pub fn style_dialog_update(&mut self, was: &StyleDialog, now: StyleDialog, fonts: &Fonts) -> Result<StyleDialog, Error> {
        let res = self.doc.definitions.clone().ok_or_else(|| Error::unavailable("no styles"))?;
        let typed = res.typed();
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
        let start = self.pending(was)?;
        // A table style's part: the formatting commands, fill, alignment and border edit it.
        let part = if table { region(was.table_part) } else { None };
        self.style_part = part;
        let formatted = if commands.is_empty() { Ok(start) } else { self.format_style(&start, commands, fonts) };
        self.style_part = None;
        let mut pending = formatted?;
        if table {
            pending = table_style_edits(&pending, was, &now, part);
        }
        pending = dialog_fields(&pending, was, &now);
        if now.kind != was.kind && was.open == StyleOpen::Create {
            let id = pending.get().id.to_string();
            pending = retyped(&pending, now.kind);
            let kind = kind_of(now.kind);
            let sheet = &*typed.cascade.styles;
            let mut next = (*res).clone();
            let base_of = next.set_style(pending.clone()).ok().and_then(|_| {
                let c = next.cascade();
                c.styles.styles.get(&*id).and_then(|e| c.based_on(e).map(Rc::<str>::from))
            });
            if base_of.is_some_and(|b| sheet.get(&b).is_none_or(|b| b.kind != kind)) {
                let base = default_base(sheet, kind);
                pending = edit_style(&pending, |s, set| {
                    s.based_on = base.as_deref().map(Rc::from);
                    set.push(StyleSet::Style(StyleProp::BasedOn));
                });
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
        let Some(mut res) = self.doc.definitions.clone() else { return Outcome::Unchanged("no styles") };
        if let StyleOpen::Modify(id) = &v.open
            && let Some(def) = built_in_definition(&res.typed(), &id.0)
        {
            let _ = std::rc::Rc::make_mut(&mut res).set_style(def);
        }
        let typed = res.typed();
        let c = &*typed.cascade;
        let styles = &*c.styles;
        let Some(start) = typed.styles.style_from_xml(&v.pending).map(|s| StyleDef::read(s, v.pending.as_str().into())) else {
            return Outcome::invalid("not a style");
        };
        let mut next = (*res).clone();
        if next.set_style(start.clone()).is_err() {
            return Outcome::invalid("not a style");
        }
        let pending_id = start.get().id.clone();
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
        let mut def = start;
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
            def = edit_style(&def, |s, set| set_names(s, set, &names.join(",")));
        }
        let creating = v.open == StyleOpen::Create;
        // A new style takes its id from its name; its following style, if it was itself, follows.
        let id = if creating {
            let id = id_for(&first, styles);
            let follows = style.next.as_deref() == Some(&*style.id);
            def = edit_style(&def, |s, set| {
                made_as(s, set, v.kind, &id);
                if follows {
                    s.next = Some(id.as_str().into());
                    set.push(StyleSet::Style(StyleProp::Next));
                }
            });
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
            def = edit_style(&def, |s, set| {
                s.link = Some(char_id.as_str().into());
                set.push(StyleSet::Style(StyleProp::Link));
            });
        }
        let char = def.get().format.char.clone();
        let half = linked.then(|| StyleDef::typed(Rc::new(char_half(&char_id, &first, &id, char.clone()))));
        // A linked style's character half takes its character formatting.
        let half = half.or_else(|| {
            let half = style
                .link
                .as_deref()
                .filter(|l| styles.get(l).is_some_and(|c| c.kind == StyleKind::Character && c.link.as_deref() == Some(id.as_str())))?;
            let was = styles.get(id.as_str()).map_or_else(Default::default, |s| s.format.char.clone());
            let theirs = style_holder(&typed, half)?;
            (!creating && style.kind == StyleKind::Paragraph && char != was).then(|| edit_style(&theirs, |s, _| s.format.char = char))
        });
        self.joined(label, fonts, |e| {
            let out = e.edit_styles(
                label,
                |s| {
                    let mut changed = s.set(def)?;
                    if let Some(c) = half {
                        changed.extend(s.set(c)?);
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

    /// The style dialog after a Format menu dialog's commands: the style changed by them, and the values read again.
    pub fn style_dialog_format(&mut self, v: &StyleDialog, commands: Vec<Command>, fonts: &Fonts) -> Result<StyleDialog, Error> {
        super::styles::formats_style(&commands)?;
        let start = self.pending(v)?;
        self.style_part = if v.kind == StyleDialogType::Table { region(v.table_part) } else { None };
        let formatted = self.format_style(&start, commands, fonts);
        self.style_part = None;
        let pending = formatted?;
        self.doc.definitions.as_ref().ok_or_else(|| Error::unavailable("no styles"))?;
        let mut out = self.style_values(&pending, v.open.clone(), v.table_part)?;
        (out.short_form, out.kind, out.name) = (v.short_form, v.kind, v.name.clone());
        if v.kind == StyleDialogType::List && !v.list_levels.is_empty() {
            (out.list_levels, out.list_level) = (v.list_levels.clone(), v.list_level);
        }
        Ok(out)
    }

    /// The style dialog with its style's paragraphs in list `num`, or in none.
    pub fn style_dialog_numbering(&mut self, v: &StyleDialog, num: Option<i64>) -> Result<StyleDialog, Error> {
        self.doc.definitions.as_ref().ok_or_else(|| Error::unavailable("no styles"))?;
        let pending = numbered(&self.pending(v)?, num);
        let mut out = self.style_values(&pending, v.open.clone(), v.table_part)?;
        (out.short_form, out.kind, out.name) = (v.short_form, v.kind, v.name.clone());
        Ok(out)
    }

    /// A table style's table, cell and row properties: its region's, or its whole table's.
    fn table_region_of(
        &self,
        raw: &str,
        part: Option<RegionKind>,
    ) -> Option<(crate::docx::props::TblProps, crate::docx::props::TcProps, crate::docx::props::TrProps)> {
        use crate::docx::table_props::{tbl_props, tc_props, tr_props};
        let def = self.style_of_xml(raw)?;
        let mut next = (**self.doc.definitions.as_ref()?).clone();
        next.set_style(def.clone()).ok()?;
        let c = next.cascade();
        let f = c.table_region(Some(&def.get().id), &part.into_iter().collect::<Vec<_>>()).format;
        Some((tbl_props(&f.table, &c.colours), tc_props(&f.cell, &c.colours), tr_props(&f.row)))
    }

    /// Table Properties' values for a table style (its XML, as the style dialog carries it): its alignment, indent,
    /// cell margins, row breaking and cell alignment (what a style holds; the rest unavailable).
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

    /// Banding's values for a table style (its XML, as the style dialog carries it).
    pub fn style_banding(&self, raw: &str) -> Option<crate::api::services::BandingDialog> {
        let (tbl, _, _) = self.table_region_of(raw, None)?;
        Some(crate::api::services::BandingDialog {
            row_band: tbl.row_band.unwrap_or(1).max(1) as u32,
            column_band: tbl.col_band.unwrap_or(1).max(1) as u32,
        })
    }

    /// The style dialog after a table style's Format entry (Borders and Shading, Table Properties, Banding): what
    /// changed from `start` set on the style, and the values read again.
    pub fn style_dialog_table_format(
        &mut self,
        v: &StyleDialog,
        start: &crate::api::services::DialogValues,
        now: &crate::api::services::DialogValues,
    ) -> Result<StyleDialog, Error> {
        self.doc.definitions.as_ref().ok_or_else(|| Error::unavailable("no styles"))?;
        let def = table_format_edits(self.pending(v)?, region(v.table_part), start, now)?;
        let mut out = self.style_values(&def, v.open.clone(), v.table_part)?;
        (out.short_form, out.kind, out.name) = (v.short_form, v.kind, v.name.clone());
        Ok(out)
    }
}

/// A table style with what changed from `start` to `now` in a Format entry (Borders and Shading, Table Properties,
/// Banding) set on it: for the part `part`, or the whole table.
pub(super) fn table_format_edits(
    def: StyleDef,
    part: Option<RegionKind>,
    start: &crate::api::services::DialogValues,
    now: &crate::api::services::DialogValues,
) -> Result<StyleDef, Error> {
    use crate::api::services::DialogValues;
    let mut def = def;
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
            let cross =
                |was: &Option<crate::api::command::Border>, now: &Option<crate::api::command::Border>| (was != now).then(|| dialog_border(now));
            let (down, up) = (cross(&a.diagonal_down, &b.diagonal_down), cross(&a.diagonal_up, &b.diagonal_up));
            let sides = [&b.top, &b.left, &b.bottom, &b.right, &b.between, &b.inside_vertical].map(dialog_border);
            let fill = match b.fill {
                NoneOr::Value(c) => Some(c),
                NoneOr::None => None,
            };
            let shaded = (a.fill, a.pattern, a.pattern_color) != (b.fill, b.pattern, b.pattern_color);
            def = edit_style(&def, |s, set| {
                if edged && part.is_none() {
                    edit_tables(s, set, None, |e| {
                        e.borders = borders_of(sides);
                        e.whole(TableProp::Borders);
                    });
                }
                // A part's cells take the diagonals with their edges; the whole table's go in its cells' properties.
                if (edged && part.is_some()) || down.is_some() || up.is_some() {
                    edit_cells(s, set, part, |e| {
                        if edged && part.is_some() {
                            for (side, x) in SIDES.into_iter().zip(sides) {
                                e.border(side, Some(x));
                            }
                        }
                        for (side, x) in [(TableSide::Tl2br, down), (TableSide::Tr2bl, up)] {
                            if let Some(x) = x {
                                e.border(side, Some(x));
                            }
                        }
                    });
                }
                if shaded {
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
                    edit_cells(s, set, part, |e| {
                        e.shading = (!clear).then(|| super::table_format::authored_shading(&sh.shd_over(e.shading.map(|s| s.pattern), None)));
                        e.whole(CellProp::Shading);
                    });
                }
            });
        }
        (DialogValues::TableProperties(a), DialogValues::TableProperties(b)) => {
            let dxa = |p: crate::api::value::Points| Some(Width::Dxa(super::table_format::twips(p.0)));
            if a.alignment != b.alignment || a.indent != b.indent || a.cell_margins != b.cell_margins {
                let jc = match b.alignment {
                    crate::api::value::HorizontalAlignment::Left => Jc::Left,
                    crate::api::value::HorizontalAlignment::Center => Jc::Center,
                    crate::api::value::HorizontalAlignment::Right => Jc::Right,
                };
                let m = &b.cell_margins;
                use super::commands::bounds::{MAX, within};
                within("the indent", b.indent.0, -MAX, MAX)?;
                for v in [m.top, m.left, m.bottom, m.right] {
                    within("the cell margin", v.0, 0.0, MAX)?;
                }
                let margins = CellMargins { top: dxa(m.top), left: dxa(m.left), bottom: dxa(m.bottom), right: dxa(m.right) };
                def = edit_style(&def, |s, set| {
                    edit_tables(s, set, None, |e| {
                        e.align = Some(jc);
                        e.whole(TableProp::Align);
                        e.indent = dxa(b.indent);
                        e.whole(TableProp::Indent);
                        e.margins = margins;
                        e.whole(TableProp::Margins);
                    })
                });
            }
            if let (Tri::Uniform(x), Tri::Uniform(y)) = (&a.row_can_break, &b.row_can_break)
                && x != y
            {
                let breaks = *y;
                def = edit_style(&def, |s, set| {
                    edit_rows(s, set, None, |e| {
                        e.cant_split = (!breaks).then_some(true);
                        e.whole(RowProp::CantSplit);
                    })
                });
            }
            if let (Tri::Uniform(x), Tri::Uniform(y)) = (&a.cell_alignment, &b.cell_alignment)
                && x != y
            {
                let va = super::table_format::cell_alignment(*y);
                def = edit_style(&def, |s, set| {
                    edit_cells(s, set, part, |e| {
                        e.v_align = Some(va);
                        e.whole(CellProp::VAlign);
                    })
                });
            }
        }
        (DialogValues::Banding(a), DialogValues::Banding(b)) if a != b => {
            let (rows, cols) = (b.row_band.max(1), b.column_band.max(1));
            def = edit_style(&def, |s, set| {
                edit_tables(s, set, None, |e| {
                    (e.row_band, e.col_band) = (Some(i64::from(rows)), Some(i64::from(cols)));
                    e.whole(TableProp::RowBand);
                    e.whole(TableProp::ColBand);
                })
            });
        }
        _ => {}
    }
    Ok(def)
}
