//! Style definitions

use std::rc::Rc;

use crate::api::command::{CellMargins, Command};
use crate::api::error::Error;
use crate::api::services::{StyleVisibility, TableStylePart};
use crate::api::state::{CharacterFormatting, ParagraphFormatting, Tri};
pub use crate::api::styles::{
    BuiltInStyle, DefaultsDefinition, NewStyle, StyleCategory, StyleChange, StyleDefinition, StyleList, TableBorders, TablePartDefinition,
    TablePartFormatting, TableStyleDefinition,
};
use crate::api::value::{Color, HorizontalAlignment, Points, StyleId};
use crate::docx::props::{ParaProps, RunProps, toggle};
use crate::docx::styles::{Style, StyleKind, Styles};
use crate::docx::write::props::{Cx, Props, STYLE, TBLPR, TC_BORDERS, TCPR, TRPR, esc};
use crate::font::Fonts;
use crate::services::styles::{NameRefusal, builtin, check_name, describe};

use super::Editor;
use super::commands::Outcome;
use crate::document::paragraph::{ListId, ListRef};
use crate::docx::run_xml;

impl StyleCategory {
    fn kind(self) -> StyleKind {
        match self {
            StyleCategory::Paragraph | StyleCategory::Linked => StyleKind::Paragraph,
            StyleCategory::Character => StyleKind::Character,
            StyleCategory::Table => StyleKind::Table,
            StyleCategory::List => StyleKind::Numbering,
        }
    }

    fn of(s: &Style) -> StyleCategory {
        match s.kind {
            StyleKind::Paragraph if s.link.is_some() => StyleCategory::Linked,
            StyleKind::Paragraph => StyleCategory::Paragraph,
            StyleKind::Character => StyleCategory::Character,
            StyleKind::Table => StyleCategory::Table,
            StyleKind::Numbering => StyleCategory::List,
        }
    }
}

const fn not_set<T>() -> Tri<T> {
    Tri::Unavailable
}

/// `all` where the run properties set it.
pub(super) fn own_character(r: &RunProps, all: &CharacterFormatting) -> CharacterFormatting {
    fn keep<T: Clone>(set: bool, t: &Tri<T>) -> Tri<T> {
        if set { t.clone() } else { not_set() }
    }
    let bit = |b: u16| r.toggles_set & b != 0;
    CharacterFormatting {
        font: keep(r.ascii.is_some() || r.h_ansi.is_some(), &all.font),
        size: keep(r.size.is_some(), &all.size),
        bold: keep(bit(toggle::B), &all.bold),
        italic: keep(bit(toggle::I), &all.italic),
        underline: keep(r.underline.is_some(), &all.underline),
        strikethrough: keep(bit(toggle::STRIKE), &all.strikethrough),
        double_strikethrough: keep(r.dstrike.is_some(), &all.double_strikethrough),
        subscript: keep(r.vert_align.is_some(), &all.subscript),
        superscript: keep(r.vert_align.is_some(), &all.superscript),
        small_caps: keep(bit(toggle::SMALL_CAPS), &all.small_caps),
        all_caps: keep(bit(toggle::CAPS), &all.all_caps),
        hidden: keep(bit(toggle::VANISH), &all.hidden),
        color: keep(r.color.is_some(), &all.color),
        highlight: keep(r.highlight.is_some(), &all.highlight),
        character_spacing: keep(r.spacing.is_some(), &all.character_spacing),
        position: keep(r.position.is_some(), &all.position),
        character_style: not_set(),
        scale: keep(r.scale.is_some(), &all.scale),
        kerning: keep(r.kern.is_some(), &all.kerning),
        ligatures: keep(r.ligatures.is_some(), &all.ligatures),
        number_spacing: keep(r.num_spacing.is_some(), &all.number_spacing),
        number_form: keep(r.num_form.is_some(), &all.number_form),
        stylistic_set: keep(r.style_sets.is_some(), &all.stylistic_set),
        contextual_alternates: keep(r.cntxt_alts.is_some(), &all.contextual_alternates),
        language: keep(r.lang.is_some(), &all.language),
        no_proof: keep(r.no_proof.is_some(), &all.no_proof),
        emboss: keep(bit(toggle::EMBOSS), &all.emboss),
        engrave: keep(bit(toggle::IMPRINT), &all.engrave),
        outline: keep(bit(toggle::OUTLINE), &all.outline),
        shadow: keep(bit(toggle::SHADOW), &all.shadow),
        underline_color: keep(r.underline_color.is_some(), &all.underline_color),
        text_effects: keep(r.effects.is_some(), &all.text_effects),
        font_theme: keep(r.ascii.is_some() || r.h_ansi.is_some(), &all.font_theme),
        color_value: keep(r.color.is_some(), &all.color_value),
        underline_color_value: keep(r.underline_color.is_some(), &all.underline_color_value),
        text_border: keep(r.border.is_some(), &all.text_border),
        text_shading: keep(r.shd.is_some() || r.shading.is_some(), &all.text_shading),
        fit_text: keep(r.fit_text.is_some(), &all.fit_text),
        ligature_flags: keep(r.ligatures.is_some(), &all.ligature_flags),
        stylistic_sets: keep(r.style_sets.is_some(), &all.stylistic_sets),
    }
}

/// `all` where the paragraph properties set it.
pub(super) fn own_paragraph(p: &ParaProps, all: &ParagraphFormatting) -> ParagraphFormatting {
    fn keep<T: Clone>(set: bool, t: &Tri<T>) -> Tri<T> {
        if set { t.clone() } else { not_set() }
    }
    let borders = [&p.border_top, &p.border_bottom, &p.border_left, &p.border_right, &p.border_between].iter().any(|b| b.is_some());
    ParagraphFormatting {
        style: not_set(),
        alignment: keep(p.jc.is_some(), &all.alignment),
        line_spacing: keep(p.line.is_some() || p.line_rule.is_some(), &all.line_spacing),
        space_before: keep(p.space_before.is_some() || p.before_auto.is_some(), &all.space_before),
        space_after: keep(p.space_after.is_some() || p.after_auto.is_some(), &all.space_after),
        indents: crate::api::state::IndentState {
            left: keep(p.ind_left.is_some(), &all.indents.left),
            right: keep(p.ind_right.is_some(), &all.indents.right),
            first_line: keep(p.ind_first.is_some(), &all.indents.first_line),
            hanging: keep(p.ind_first.is_some(), &all.indents.hanging),
            mirror: keep(p.mirror_indents.is_some(), &all.indents.mirror),
        },
        list: keep(p.num_id.is_some(), &all.list),
        list_level: keep(p.ilvl.is_some() || p.num_id.is_some(), &all.list_level),
        flow: crate::api::state::FlowState {
            keep_with_next: keep(p.keep_next.is_some(), &all.flow.keep_with_next),
            keep_lines_together: keep(p.keep_lines.is_some(), &all.flow.keep_lines_together),
            page_break_before: keep(p.page_break_before.is_some(), &all.flow.page_break_before),
            widow_control: keep(p.widow_control.is_some(), &all.flow.widow_control),
            dont_hyphenate: keep(p.suppress_auto_hyphens.is_some(), &all.flow.dont_hyphenate),
            suppress_line_numbers: keep(p.suppress_line_numbers.is_some(), &all.flow.suppress_line_numbers),
            textbox_tight_wrap: keep(p.tight_wrap.is_some(), &all.flow.textbox_tight_wrap),
        },
        borders: keep(borders, &all.borders),
        shading: keep(p.shading.is_some() || p.shd.is_some(), &all.shading),
        contextual_spacing: keep(p.contextual.is_some(), &all.contextual_spacing),
        outline_level: keep(p.outline_lvl.is_some(), &all.outline_level),
        collapsed: keep(p.collapsed.is_some(), &all.collapsed),
        text_direction: keep(p.text_flow.is_some(), &all.text_direction),
        drop_cap: if p.frame.is_some() { all.drop_cap.clone() } else { None },
        border_top: keep(p.border_top.is_some(), &all.border_top),
        border_left: keep(p.border_left.is_some(), &all.border_left),
        border_bottom: keep(p.border_bottom.is_some(), &all.border_bottom),
        border_right: keep(p.border_right.is_some(), &all.border_right),
        border_between: keep(p.border_between.is_some(), &all.border_between),
        shading_value: keep(p.shading.is_some() || p.shd.is_some(), &all.shading_value),
        tabs: keep(!p.tabs.is_empty(), &all.tabs),
    }
}

/// Every table style part, the whole table first.
pub const TABLE_PARTS: [TableStylePart; 13] = [
    TableStylePart::WholeTable,
    TableStylePart::HeaderRow,
    TableStylePart::TotalRow,
    TableStylePart::FirstColumn,
    TableStylePart::LastColumn,
    TableStylePart::OddBandedRows,
    TableStylePart::EvenBandedRows,
    TableStylePart::OddBandedColumns,
    TableStylePart::EvenBandedColumns,
    TableStylePart::TopLeftCell,
    TableStylePart::TopRightCell,
    TableStylePart::BottomLeftCell,
    TableStylePart::BottomRightCell,
];

/// The table commands a table style takes.
fn table_command(c: &Command) -> bool {
    matches!(
        c,
        Command::SetCellShading(_)
            | Command::SetCellBorders(_)
            | Command::SetCellAlignment(_)
            | Command::SetCellMargins(_)
            | Command::SetRowCanBreak(_)
            | Command::SetTableAlignment(_)
            | Command::SetTableIndent(_)
    )
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

/// A style id from a name: its ASCII letters and digits, numbered past the ids taken.
pub(super) fn id_for(name: &str, styles: &Styles) -> String {
    let base: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
    let base = if base.is_empty() { "Style".to_string() } else { base };
    let free = |id: &String| !styles.by_id.contains_key(id) && builtin::BUILT_IN.iter().all(|b| b.id != id);
    (0..).map(|n| if n == 0 { base.clone() } else { format!("{base}{n}") }).find(free).unwrap_or(base)
}

/// A table style edit by a table command, for the part `region` (none: the whole table).
fn table_style_edit(e: &Editor, raw: &str, region: Option<&'static str>, c: &Command, cx: &Cx) -> Result<String, Error> {
    use super::table_format::{Edges, border_xml};
    let w = cx.w.clone();
    let root = |raw: &str, child: &str, order: &'static [&'static str], edit: &dyn Fn(&mut Props)| {
        super::styles::edit_child(raw, child, cx, |p| {
            p.reorder(order);
            edit(p);
        })
    };
    let cell = |raw: &str, edit: &dyn Fn(&mut Props)| match region {
        Some(r) => super::styles::edit_table_part(raw, r, "tcPr", cx, |p| {
            p.reorder(TCPR);
            edit(p);
        }),
        None => root(raw, "tcPr", TCPR, edit),
    };
    let tw = |p: Points| (p.0 * 20.0).round() as i64;
    let whole_only = |what: &str| if region.is_some() { Err(Error::invalid(format!("{what} is the whole table's"))) } else { Ok(()) };
    Ok(match c {
        Command::SetCellShading(fill) => {
            let fill = fill.as_ref().map(|c| e.word_colour(c)).transpose().map_err(Error::invalid)?;
            cell(raw, &|p| match fill {
                Some(c) => {
                    let tag = format!("<{w}shd {w}val=\"clear\" {w}color=\"auto\"/>");
                    p.set("shd", super::commands::colour_attrs(&tag, &w, "fill", super::commands::THEME_FILL, Some(c)))
                }
                None => p.remove("shd"),
            })
        }
        Command::SetCellAlignment(a) => {
            let v = super::table_format::v_align_xml(&w, super::table_format::cell_alignment(*a));
            cell(raw, &|p| p.set("vAlign", v.clone()))
        }
        Command::SetCellMargins(m) => {
            let sides = [("top", m.top), ("left", m.left), ("bottom", m.bottom), ("right", m.right)];
            let edit = |p: &mut Props, name: &str| {
                let mut mar = Props::parse(p.get(name), name, cx);
                mar.reorder(&["top", "start", "left", "bottom", "end", "right"]);
                for (side, v) in sides {
                    if let Some(v) = v {
                        mar.set(side, format!("<{w}{side} {w}w=\"{}\" {w}type=\"dxa\"/>", tw(v)));
                    }
                }
                match mar.xml() {
                    Some(x) => p.set(name, x),
                    None => p.remove(name),
                }
            };
            match region {
                Some(_) => cell(raw, &|p| edit(p, "tcMar")),
                None => root(raw, "tblPr", TBLPR, &|p| edit(p, "tblCellMar")),
            }
        }
        Command::SetRowCanBreak(on) => match region {
            // A part's own setting, over the whole table's
            Some(r) => super::styles::edit_table_part(raw, r, "trPr", cx, |p| {
                p.reorder(TRPR);
                p.set("cantSplit", if *on { format!("<{w}cantSplit {w}val=\"0\"/>") } else { format!("<{w}cantSplit/>") });
            }),
            // Said either way: a base style's setting would show through its absence.
            None => root(raw, "trPr", TRPR, &|p| {
                p.set("cantSplit", if *on { format!("<{w}cantSplit {w}val=\"0\"/>") } else { format!("<{w}cantSplit/>") })
            }),
        },
        Command::SetTableAlignment(a) => {
            whole_only("the alignment")?;
            let v = match a {
                HorizontalAlignment::Left => "left",
                HorizontalAlignment::Center => "center",
                HorizontalAlignment::Right => "right",
            };
            root(raw, "tblPr", TBLPR, &|p| p.set("jc", format!("<{w}jc {w}val=\"{v}\"/>")))
        }
        Command::SetTableIndent(i) => {
            whole_only("the indent")?;
            root(raw, "tblPr", TBLPR, &|p| p.set("tblInd", format!("<{w}tblInd {w}w=\"{}\" {w}type=\"dxa\"/>", tw(*i))))
        }
        Command::SetCellBorders(b) => {
            let line = b.border.as_ref().map(|l| super::readback::BorderLine::of_api(l, 0.0, false));
            let (edges, line) = e.border_lines(b.edges, line.as_ref()).map_err(Error::invalid)?;
            let nil = Edges::None == edges;
            let sides: &[&str] = match edges {
                Edges::All | Edges::None => &["top", "left", "bottom", "right", "insideH", "insideV"],
                Edges::Outside => &["top", "left", "bottom", "right"],
                Edges::Inside => &["insideH", "insideV"],
                Edges::InsideH => &["insideH"],
                Edges::InsideV => &["insideV"],
                Edges::Top => &["top"],
                Edges::Bottom => &["bottom"],
                Edges::Left => &["left"],
                Edges::Right => &["right"],
                Edges::DiagonalDown | Edges::DiagonalUp => &[],
            };
            let diagonals: &[&str] = match edges {
                Edges::DiagonalDown => &["tl2br"],
                Edges::DiagonalUp => &["tr2bl"],
                Edges::None => &["tl2br", "tr2bl"],
                _ => &[],
            };
            let xml = |side: &str| if nil { format!("<{w}{side} {w}val=\"nil\"/>") } else { border_xml(&w, side, &line) };
            let edit = |p: &mut Props, name: &str, names: &[&str]| {
                let mut bd = Props::parse(p.get(name), name, cx);
                bd.reorder(TC_BORDERS);
                for side in names {
                    bd.set(side, xml(side));
                }
                match bd.xml() {
                    Some(x) => p.set(name, x),
                    None => p.remove(name),
                }
            };
            let mut raw = raw.to_string();
            match region {
                Some(_) => {
                    let all: Vec<&str> = sides.iter().chain(diagonals).copied().collect();
                    raw = cell(&raw, &|p| edit(p, "tcBorders", &all));
                }
                None => {
                    if !sides.is_empty() {
                        raw = root(&raw, "tblPr", TBLPR, &|p| edit(p, "tblBorders", sides));
                    }
                    if !diagonals.is_empty() {
                        raw = cell(&raw, &|p| edit(p, "tcBorders", diagonals));
                    }
                }
            }
            raw
        }
        _ => return Err(Error::invalid(format!("{:?} doesn't format a table style", c.id()))),
    })
}

fn name_error(e: NameRefusal) -> Error {
    match e {
        NameRefusal::Empty => Error::invalid("a style needs a name"),
        NameRefusal::Duplicate => Error::invalid("that style name already exists"),
        NameRefusal::Reserved => Error::invalid("that style name is reserved for a built-in style"),
    }
}

/// The styles an edit adds or replaces, in order: the style, and others it changes with it (a linked style's other
/// half, built-in styles it refers to).
type Written = Vec<String>;

impl Editor {
    /// The formatting state of text in a style's XML: what a style definition reads.
    pub fn style_formatting_state(&self, raw: &str) -> Option<(CharacterFormatting, ParagraphFormatting, Vec<crate::api::services::TabStopInfo>)> {
        let (para, run, _) = self.style_formatting(raw)?;
        let tabs: Vec<_> =
            para.style.tabs.iter().filter(|t| t.kind != crate::docx::props::TabKind::Clear).map(|t| super::commands::tabs::info(t, false)).collect();
        // Its list, its own or a base style's: bulleted or numbered
        let res = self.doc.resolver.as_ref()?;
        let list = para
            .style
            .num
            .filter(|ListRef { list: ListId(n), level: _ }| *n != 0)
            .and_then(|ListRef { list: ListId(n), level: l }| res.numbering.level(n, l, &res.styles))
            .map(|l| l.fmt == "bullet");
        let mut paragraph = super::state::paragraph(&[(&para.style, list)]);
        paragraph.tabs = Tri::Uniform(tabs.clone());
        Some((super::state::character(&[&run]), paragraph, tabs))
    }

    /// Why the command gates refuse `cmd`, for an entry point that backs it: read-only mode, then document protection.
    pub(super) fn refuse_like(&mut self, cmd: &Command) -> Option<Outcome> {
        if self.mode == super::Mode::ReadOnly {
            return Some(self.refuse_read_only());
        }
        self.protection_refusal(cmd).map(Outcome::Refused)
    }

    /// A style edit refused: a read-only document, or document protection (formatting protection, or Restrict
    /// Editing), as a style update is.
    pub(super) fn refuse_style_edit(&mut self) -> Option<Outcome> {
        self.refuse_like(&Command::UpdateStyleToSelection(StyleId::default()))
    }

    /// The document's style `id`, or the built-in style it names when the document doesn't define it (`false`).
    fn style_or_built_in(&self, id: &str) -> Option<(Style, bool)> {
        let styles = &self.doc.resolver.as_ref()?.styles;
        if let Some(s) = styles.get(id) {
            return Some((s.clone(), true));
        }
        let b = builtin::BUILT_IN.iter().find(|b| b.id == id)?;
        Some((styles.style_from_xml(&builtin::definition(b, &styles.cx().w))?, false))
    }

    /// A style of kind `kind` a change may refer to: the document's, or a built-in one it adds (`Some(xml)`).
    fn referable(&self, id: &StyleId, kind: StyleKind, what: &str) -> Result<Option<String>, Error> {
        let styles = &self.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?.styles;
        match styles.get(&id.0) {
            Some(s) if s.kind == kind => Ok(None),
            Some(_) => Err(Error::invalid(format!("the {what} is a style of another type"))),
            None => match builtin::BUILT_IN.iter().find(|b| b.id == id.0 && b.kind == kind) {
                Some(b) => Ok(Some(builtin::definition(b, &styles.cx().w))),
                None => Err(Error::invalid(format!("the {what} doesn't exist"))),
            },
        }
    }

    /// Runs formatting commands on a style's XML (the part `region` of a table style, none: its own), in order: the
    /// character and paragraph ones as on text, the table ones on its table properties.
    fn format_style_with(
        &mut self,
        raw: String,
        kind: StyleKind,
        region: Option<&'static str>,
        commands: &[Command],
        fonts: &Fonts,
    ) -> Result<String, Error> {
        let cx = self.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?.styles.cx();
        for c in commands {
            super::commands::bounds::bounds(c)?;
            let table = table_command(c)
                || matches!(c, Command::SetShading(s) if s.target != crate::api::command::ShadingTarget::Paragraph && s.target != crate::api::command::ShadingTarget::Text);
            if table && kind != StyleKind::Table {
                return Err(Error::invalid(format!("only a table style takes {:?}", c.id())));
            }
            if !table && !super::styles::formats_a_style(c) {
                return Err(Error::invalid(format!("{:?} doesn't format a style", c.id())));
            }
            match kind {
                StyleKind::Numbering => return Err(Error::invalid("a list style's formatting is its levels'")),
                StyleKind::Character if !super::list_labels::formats_characters(c) => {
                    return Err(Error::invalid(format!("a character style takes character formatting, not {:?}", c.id())));
                }
                _ => {}
            }
        }
        let mut raw = raw;
        let mut k = 0;
        while k < commands.len() {
            if table_command(&commands[k]) {
                raw = table_style_edit(self, &raw, region, &commands[k], &cx)?;
                k += 1;
                continue;
            }
            let end = commands[k..].iter().position(table_command).map_or(commands.len(), |e| k + e);
            self.style_part = region;
            let out = self.format_style(&raw, commands[k..end].to_vec(), fonts);
            self.style_part = None;
            raw = out?;
            k = end;
        }
        Ok(raw)
    }

    /// A style's XML with `change` written into it: `raw` is the style as it is (or as made), `kind` its kind.
    /// Also answers the other styles to write with it.
    fn changed_style(&mut self, raw: String, kind: StyleKind, change: &StyleChange, fonts: &Fonts) -> Result<(String, Written), Error> {
        let res = self.doc.resolver.clone().ok_or_else(|| Error::unavailable("no styles"))?;
        let styles = &res.styles;
        let cx = styles.cx();
        let w = cx.w.clone();
        let mut also: Written = Vec::new();
        let para = kind == StyleKind::Paragraph;
        let refuse = |what: &str| Err(Error::invalid(format!("{what} is a paragraph style's")));
        if !para && (change.next.is_some() || change.auto_update.is_some() || change.list.is_some()) {
            return refuse("the following style, automatic updating and a list");
        }
        if kind != StyleKind::Table && (!change.parts.is_empty() || change.row_band.is_some() || change.column_band.is_some()) {
            return Err(Error::invalid("table parts and bands are a table style's"));
        }
        if kind != StyleKind::Numbering && change.list_levels.is_some() {
            return Err(Error::invalid("levels are a list style's"));
        }
        let mut raw = raw;
        if change.clear {
            raw = super::styles::edit_child(&raw, "pPr", &cx, |p| p.kids.retain(|(n, _)| n == "numPr"));
            raw = set_child(&raw, &cx, "rPr", None);
            if kind == StyleKind::Table {
                for child in ["tblPr", "trPr", "tcPr"] {
                    raw = set_child(&raw, &cx, child, None);
                }
            }
        }
        raw = self.format_style_with(raw, kind, None, &change.formatting, fonts)?;
        for part in &change.parts {
            let region = part.part.region();
            if part.clear {
                let mut el = Props::parse(Some(&raw), "style", &cx);
                match region {
                    Some(r) => {
                        el.kids.retain(|(n, x)| n != "tblStylePr" || cx.parse(x, |_, p| crate::docx::xml::attr(p, "type") != Some(r)).unwrap_or(true))
                    }
                    None => el.kids.retain(|(n, _)| !["pPr", "rPr", "tblPr", "trPr", "tcPr"].contains(&n.as_str())),
                }
                raw = el.xml().unwrap_or(raw);
            }
            raw = self.format_style_with(raw, kind, region, &part.formatting, fonts)?;
        }
        if change.row_band.is_some() || change.column_band.is_some() {
            for (n, name) in [(change.row_band, "tblStyleRowBandSize"), (change.column_band, "tblStyleColBandSize")] {
                let Some(n) = n else { continue };
                super::commands::bounds::within("the band size", n as f64, 1.0, 32767.0)?;
                raw = super::styles::edit_child(&raw, "tblPr", &cx, |p| {
                    p.reorder(TBLPR);
                    p.set(name, format!("<{w}{name} {w}val=\"{n}\"/>"));
                });
            }
        }
        let id = styles.style_from_xml(&raw).map(|s| s.id).ok_or_else(|| Error::invalid("not a style"))?;
        let val = |name: &str, v: &str| format!("<{w}{name} {w}val=\"{}\"/>", esc(v));
        if let Some(b) = &change.based_on {
            if let Some(b) = b {
                if b.0 == id {
                    return Err(Error::invalid("a style can't be based on itself"));
                }
                also.extend(self.referable(b, kind, "base")?);
            }
            raw = set_child(&raw, &cx, "basedOn", b.as_ref().map(|b| val("basedOn", &b.0)));
        }
        if let Some(n) = &change.next {
            if let Some(n) = n.as_ref().filter(|n| n.0 != id) {
                also.extend(self.referable(n, StyleKind::Paragraph, "following style")?);
            }
            raw = set_child(&raw, &cx, "next", n.as_ref().map(|n| val("next", &n.0)));
        }
        if let Some(on) = change.auto_update {
            raw = set_child(&raw, &cx, "autoRedefine", on.then(|| format!("<{w}autoRedefine/>")));
        }
        if let Some(on) = change.quick {
            raw = set_child(&raw, &cx, "qFormat", on.then(|| format!("<{w}qFormat/>")));
        }
        if let Some(l) = &change.list {
            if let Some(l) = l {
                if !res.numbering.nums.contains_key(&l.list) || l.list == 0 {
                    return Err(Error::invalid("no such list"));
                }
                if l.level > 8 {
                    return Err(Error::invalid("a list level is 0 to 8"));
                }
            }
            raw = super::styles::edit_child(&raw, "pPr", &cx, |p| match l {
                Some(StyleList { list, level: 0 }) => p.set("numPr", format!("<{w}numPr><{w}numId {w}val=\"{list}\"/></{w}numPr>")),
                Some(StyleList { list, level }) => {
                    p.set("numPr", format!("<{w}numPr><{w}ilvl {w}val=\"{level}\"/><{w}numId {w}val=\"{list}\"/></{w}numPr>"))
                }
                // No list over a base style's: `numId` 0.
                None => p.set("numPr", format!("<{w}numPr><{w}numId {w}val=\"0\"/></{w}numPr>")),
            });
        }
        if let Some(name) = &change.name {
            let first = name.trim();
            let cur = styles.style_from_xml(&raw).ok_or_else(|| Error::invalid("not a style"))?;
            if !describe::display_name(&cur).eq_ignore_ascii_case(first) {
                check_name(styles, first, Some(&id)).map_err(name_error)?;
                // A built-in style keeps its name: the new one is an alias.
                let names: Vec<String> = if cur.custom {
                    std::iter::once(first.to_string()).chain(cur.aliases.iter().cloned()).collect()
                } else {
                    [cur.name.clone(), first.to_string()].into_iter().chain(cur.aliases.iter().cloned()).collect()
                };
                raw = set_child(&raw, &cx, "name", Some(val("name", &names.join(","))));
            }
        }
        // Links: this style and its new partner name each other; their old partners name neither.
        let was_link = styles.get(&id).and_then(|s| s.link.clone());
        let mut link = was_link.clone();
        if let Some(l) = &change.link {
            let other = match kind {
                StyleKind::Paragraph => StyleKind::Character,
                StyleKind::Character => StyleKind::Paragraph,
                _ => return Err(Error::invalid("only paragraph and character styles link")),
            };
            if let Some(l) = l {
                match styles.get(&l.0) {
                    Some(s) if s.kind == other => {}
                    Some(_) => return Err(Error::invalid("a style links to one of the other type")),
                    None => return Err(Error::invalid("the linked style doesn't exist")),
                }
            }
            link = l.as_ref().map(|l| l.0.clone());
            raw = set_child(&raw, &cx, "link", link.as_ref().map(|l| val("link", l)));
            let unlink = |raw: &str| set_child(raw, &cx, "link", None);
            if let Some(old) = was_link.as_ref().filter(|o| Some(*o) != link.as_ref())
                && let Some(s) = styles.get(old).filter(|s| s.link.as_deref() == Some(&id))
            {
                also.push(unlink(&s.raw));
            }
            if let Some(new) = &link
                && let Some(s) = styles.get(new)
                && s.link.as_deref() != Some(&id)
            {
                if let Some(partner) = s.link.as_deref().filter(|p| *p != id)
                    && let Some(p) = styles.get(partner).filter(|p| p.link.as_deref() == Some(new.as_str()))
                {
                    also.push(unlink(&p.raw));
                }
                also.push(set_child(&s.raw, &cx, "link", Some(val("link", &id))));
            }
        }
        // A linked style's other half takes its character formatting.
        let rpr = |x: &str| Props::parse(Some(x), "style", &cx).get("rPr").map(str::to_string);
        let before = styles.get(&id).map(|s| rpr(&s.raw)).unwrap_or_default();
        if let Some(l) = &link
            && rpr(&raw) != before
            && matches!(kind, StyleKind::Paragraph | StyleKind::Character)
        {
            let half = also.iter().position(|x| styles.style_from_xml(x).is_some_and(|s| s.id == *l));
            let base = match half {
                Some(i) => Some(also.remove(i)),
                None => styles.get(l).filter(|s| s.link.as_deref() == Some(&id)).map(|s| s.raw.clone()),
            };
            if let Some(base) = base {
                also.push(set_child(&base, &cx, "rPr", rpr(&raw)));
            }
        }
        Ok((raw, also))
    }

    /// Writes a style and the styles with it as one undo step, then a list style's levels in the same step.
    fn write_style(&mut self, label: &'static str, raw: String, also: Written, kind: StyleKind, change: &StyleChange, fonts: &Fonts) -> Outcome {
        let id = self.doc.resolver.as_ref().and_then(|r| r.styles.style_from_xml(&raw)).map(|s| s.id).unwrap_or_default();
        let relabel = change.list.is_some();
        let levels = change.list_levels.clone();
        // Styles it adds go first (a built-in base is there when it is read), then it, then those it changes.
        let (fresh, rest): (Vec<String>, Vec<String>) = also.into_iter().partition(|x| {
            self.doc.resolver.as_ref().is_some_and(|r| r.styles.style_from_xml(x).is_some_and(|st| !r.styles.by_id.contains_key(&st.id)))
        });
        self.joined(label, fonts, |e| {
            let out = e.edit_styles_relabel(
                label,
                |s| {
                    let mut changed = Vec::new();
                    for x in fresh {
                        changed.extend(s.set_xml(x)?);
                    }
                    changed.extend(s.set_xml(raw)?);
                    for x in rest {
                        changed.extend(s.set_xml(x)?);
                    }
                    Ok(changed)
                },
                relabel,
                fonts,
            );
            if matches!(out, Outcome::Refused(_)) || kind != StyleKind::Numbering {
                return out;
            }
            match levels {
                Some(l) => match e.define_style_list(&id, &l, fonts) {
                    o @ Outcome::Refused(_) => o,
                    o if matches!(out, Outcome::Unchanged(_)) => o,
                    _ => out,
                },
                None => out,
            }
        })
    }

    /// Changes style `id` (a built-in one the document doesn't define joins it): its name, base, following style,
    /// link, flags, list, and formatting, as one undo step. Every paragraph, run and table in it lays out again.
    pub fn modify_style(&mut self, id: &StyleId, change: StyleChange, fonts: &Fonts) -> Outcome {
        let Some((style, defined)) = self.style_or_built_in(&id.0) else { return Outcome::Refused(Error::unavailable("no such style")) };
        let raw = style.raw.clone();
        if let Some(o) = self.refuse_style_edit() {
            return o;
        }
        let (raw, also) = match self.changed_style(raw.clone(), style.kind, &change, fonts) {
            Ok(x) => x,
            Err(e) => return Outcome::Refused(e),
        };
        if defined && raw == style.raw && also.is_empty() && change.list_levels.is_none() {
            return Outcome::Unchanged("the style already is so");
        }
        self.write_style("Modify Style", raw, also, style.kind, &change, fonts)
    }

    /// Makes a style, as one undo step (applied too, when asked); its id, made from its name.
    pub fn create_style(&mut self, new: NewStyle, fonts: &Fonts) -> (Outcome, Option<StyleId>) {
        if let Some(o) = self.refuse_style_edit() {
            return (o, None);
        }
        let Some(res) = self.doc.resolver.clone() else { return (Outcome::Unchanged("no styles"), None) };
        let styles = &res.styles;
        let cx = styles.cx();
        let w = cx.w.clone();
        let name = match &new.change.name {
            Some(n) => n.trim().to_string(),
            None => crate::services::styles::next_new_name(styles),
        };
        if let Err(e) = check_name(styles, &name, None) {
            return (Outcome::Refused(name_error(e)), None);
        }
        let kind = new.category.kind();
        let id = id_for(&name, styles);
        let xml_type = match kind {
            StyleKind::Paragraph => "paragraph",
            StyleKind::Character => "character",
            StyleKind::Table => "table",
            StyleKind::Numbering => "numbering",
        };
        // Based on the selection's paragraph style (a paragraph style), or the default one of its type.
        let para = self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para(&self.doc, p));
        let base = match kind {
            StyleKind::Paragraph if new.from_selection => para.and_then(|p| p.style.style_id.clone()).or_else(|| styles.default_paragraph.clone()),
            StyleKind::Paragraph => styles.default_paragraph.clone(),
            StyleKind::Character => {
                Some("DefaultParagraphFont".to_string()).filter(|d| styles.get(d).is_some_and(|s| s.kind == StyleKind::Character))
            }
            StyleKind::Table => styles.default_table.clone(),
            StyleKind::Numbering => None,
        };
        let (ppr, rpr) = if new.from_selection { self.selection_formatting() } else { (None, None) };
        let (ppr, rpr) = match kind {
            StyleKind::Paragraph => (ppr, rpr),
            StyleKind::Character => (None, rpr),
            _ => (None, None),
        };
        let base = base.map_or(String::new(), |b| format!("<{w}basedOn {w}val=\"{}\"/>", esc(&b)));
        let next = if kind == StyleKind::Paragraph { format!("<{w}next {w}val=\"{}\"/>", esc(&id)) } else { String::new() };
        let raw = format!(
            "<{w}style {w}type=\"{xml_type}\" {w}customStyle=\"1\" {w}styleId=\"{i}\"><{w}name {w}val=\"{n}\"/>{base}{next}<{w}qFormat/>{}{}</{w}style>",
            ppr.unwrap_or_default(),
            rpr.unwrap_or_default(),
            i = esc(&id),
            n = esc(&name)
        );
        let mut raw = match styles.style_from_xml(&raw) {
            Some(_) => raw,
            None => return (Outcome::Refused(Error::invalid("not a style")), None),
        };
        let mut change = new.change.clone();
        change.name = None;
        if new.category == StyleCategory::Linked && change.link == Some(None) {
            return (Outcome::Refused(Error::invalid("a linked style is linked to a character style")), None);
        }
        // A linked style: its character half, made with it.
        let mut half = None;
        if new.category == StyleCategory::Linked && change.link.is_none() {
            let char_id = (0..)
                .map(|n| if n == 0 { format!("{id}Char") } else { format!("{id}Char{n}") })
                .find(|c| !styles.by_id.contains_key(c))
                .unwrap_or_default();
            raw = set_child(&raw, &cx, "link", Some(format!("<{w}link {w}val=\"{}\"/>", esc(&char_id))));
            half = Some(char_id);
        }
        let label = "New Style";
        let apply = new.apply;
        let category = new.category;
        let out = self.joined(label, fonts, |e| {
            // The style alone first, so the change refers to it as it is.
            let out = e.edit_styles(label, |s| s.set_xml(raw.clone()), fonts);
            if matches!(out, Outcome::Refused(_)) {
                return out;
            }
            let (raw, mut also) = match e.changed_style(raw.clone(), kind, &change, fonts) {
                Ok(x) => x,
                Err(err) => return Outcome::Refused(err),
            };
            if let Some(c) = &half {
                let rpr = Props::parse(Some(&raw), "style", &cx).get("rPr").map(str::to_string).unwrap_or_default();
                also.push(format!(
                    "<{w}style {w}type=\"character\" {w}customStyle=\"1\" {w}styleId=\"{c}\"><{w}name {w}val=\"{n} Char\"/><{w}basedOn {w}val=\"DefaultParagraphFont\"/><{w}link {w}val=\"{i}\"/>{rpr}</{w}style>",
                    c = esc(c),
                    n = esc(&name),
                    i = esc(&id)
                ));
            }
            let written = e.write_style(label, raw, also, kind, &change, fonts);
            if matches!(written, Outcome::Refused(_)) || !apply {
                return written;
            }
            match category {
                StyleCategory::Table => match e.table_target() {
                    Some(_) => e.execute(Command::SetTableStyle(StyleId(id.clone())), fonts),
                    None => written,
                },
                StyleCategory::List => {
                    // Its numbering its own or a base's (a list style's chain)
                    let entry = crate::api::services::ListEntryId(format!("style:{id}"));
                    if e.list_entry(&entry).is_none() {
                        return Outcome::Refused(Error::invalid("the list style has no numbering to apply: give it list_levels"));
                    }
                    e.apply_list_entry(&entry, fonts)
                }
                StyleCategory::Character => e.format(&super::format::Format::CharStyle(id.clone()), fonts),
                _ => e.format(&super::format::Format::ParaStyle(id.clone()), fonts),
            }
        });
        let made = self.doc.resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key(&id));
        (out, made.then(|| StyleId(id.clone())))
    }

    /// Style `id` in full, without a dialog: its names, type, relations, flags, list, and formatting resolved and as
    /// set by it, with a table style's parts and a list style's levels. A built-in style the document doesn't define
    /// reads as it would be once used.
    pub fn style_definition(&self, id: &StyleId) -> Option<StyleDefinition> {
        let res = self.doc.resolver.as_ref()?;
        let (style, defined) = self.style_or_built_in(&id.0)?;
        let mut styles = res.styles.clone();
        if !defined {
            styles.set_xml(style.raw.clone()).ok()?;
        }
        let style = styles.get(&id.0)?.clone();
        let (character, paragraph, tabs) = match style.kind {
            StyleKind::Table => {
                let mut next = (**res).clone();
                next.styles = styles.clone();
                let region = next.cascade().table_region(Some(&style.id), &[]);
                let para = crate::docx::para_xml::resolve(&next, &Default::default(), None, Some(&region));
                let run = Rc::new(run_xml::resolve(&next, &para.base, Default::default(), false));
                let tabs = para
                    .style
                    .tabs
                    .iter()
                    .filter(|t| t.kind != crate::docx::props::TabKind::Clear)
                    .map(|t| super::commands::tabs::info(t, false))
                    .collect();
                (super::state::character(&[&run]), super::state::paragraph(&[(&para.style, None)]), tabs)
            }
            _ => self.style_formatting_state(&style.raw)?,
        };
        let own_character = own_character(&style.base.rpr, &character);
        let own_paragraph =
            if style.kind == StyleKind::Character { ParagraphFormatting::default() } else { own_paragraph(&style.base.ppr, &paragraph) };
        let list = style
            .base
            .ppr
            .num_id
            .filter(|n| *n != 0 && style.kind == StyleKind::Paragraph)
            .map(|list| StyleList { list, level: style.base.ppr.ilvl.unwrap_or(0).clamp(0, 8) as u8 });
        let list_levels = match style.kind {
            StyleKind::Numbering => {
                styles.chain_ppr(Some(&style.id)).num_id.filter(|n| *n != 0).and_then(|n| self.list_levels(n)).unwrap_or_default()
            }
            _ => Vec::new(),
        };
        let table = (style.kind == StyleKind::Table).then(|| self.table_style_definition(&styles, &style.id)).flatten();
        Some(StyleDefinition {
            id: StyleId(style.id.clone()),
            name: describe::display_name(&style),
            aliases: style.aliases.clone(),
            category: StyleCategory::of(&style),
            built_in: !style.custom,
            defined,
            based_on: style.based_on.clone().map(StyleId),
            next: style.next.clone().filter(|n| *n != style.id).map(StyleId),
            link: style.link.clone().filter(|l| styles.by_id.contains_key(l)).map(StyleId),
            auto_update: style.meta.auto_redefine,
            quick: style.quick,
            visibility: match (style.meta.semi_hidden || style.meta.hidden, style.meta.unhide_when_used) {
                (true, true) => StyleVisibility::HideUntilUsed,
                (true, false) => StyleVisibility::Hide,
                (false, _) => StyleVisibility::Show,
            },
            priority: style.priority.unwrap_or(99),
            locked: style.meta.locked,
            list,
            character,
            paragraph,
            tabs,
            own_character,
            own_paragraph,
            description: describe::describe(&style, &styles),
            table,
            list_levels,
        })
    }

    /// A table style's table formatting and its parts, read with `styles` (which holds it).
    fn table_style_definition(&self, styles: &Styles, id: &str) -> Option<TableStyleDefinition> {
        let res = self.doc.resolver.as_ref()?;
        let mut next = (**res).clone();
        next.styles = styles.clone();
        let whole = next.styles.table_region(Some(id), &[]);
        let line = |b: &Option<crate::docx::props::Border>| super::borders_dialog::line(b);
        let borders = |b: &crate::docx::props::Borders| TableBorders {
            top: line(&b.top),
            left: line(&b.left),
            bottom: line(&b.bottom),
            right: line(&b.right),
            inside_horizontal: line(&b.inside_h),
            inside_vertical: line(&b.inside_v),
            diagonal_down: line(&b.tl2br),
            diagonal_up: line(&b.tr2bl),
        };
        let margins = |m: &crate::docx::props::Margins| CellMargins {
            top: m.top.map(Points),
            bottom: m.bottom.map(Points),
            left: m.left.map(Points),
            right: m.right.map(Points),
        };
        let parts = TABLE_PARTS
            .iter()
            .map(|&part| {
                let region = part.region();
                let r = next.styles.table_region(Some(id), &region.into_iter().collect::<Vec<_>>());
                let typed = next.cascade().table_region(Some(id), &crate::docx::style_xml::region_kinds(region));
                let para = crate::docx::para_xml::resolve(&next, &Default::default(), None, Some(&typed));
                let run = Rc::new(run_xml::resolve(&next, &para.base, Default::default(), false));
                let mut b = borders(&r.tc.borders);
                if region.is_none() {
                    b = TableBorders { diagonal_down: b.diagonal_down, diagonal_up: b.diagonal_up, ..borders(&whole.tbl.borders) };
                }
                TablePartDefinition {
                    part,
                    defined: region.is_none_or(|g| next.styles.table_defines(Some(id), g)),
                    character: super::state::character(&[&run]),
                    paragraph: super::state::paragraph(&[(&para.style, None)]),
                    fill: r.tc.shading.flatten().map(Color),
                    cell_alignment: super::table_format::api_cell_alignment(r.tc.v_align),
                    borders: b,
                    cell_margins: if region.is_none() { margins(&whole.tbl.margins) } else { margins(&r.tc.margins) },
                    row_can_break: !r.tr.cant_split.unwrap_or(false),
                }
            })
            .collect();
        Some(TableStyleDefinition {
            alignment: match whole.tbl.jc {
                Some(crate::docx::props::Jc::Center) => HorizontalAlignment::Center,
                Some(crate::docx::props::Jc::Right) => HorizontalAlignment::Right,
                _ => HorizontalAlignment::Left,
            },
            indent: Points(whole.tbl.ind.map_or(0.0, crate::docx::props::Width::dxa)),
            fill: whole.tbl.shd.as_ref().and_then(crate::docx::props::Shd::flat).map(Color),
            row_band: whole.tbl.row_band.unwrap_or(1).clamp(1, u32::MAX as i64) as u32,
            column_band: whole.tbl.col_band.unwrap_or(1).clamp(1, u32::MAX as i64) as u32,
            parts,
        })
    }

    /// Every built-in style, with whether the document defines it and the flags style lists use: the document's own
    /// for those it defines, its latent styles' for the rest (Word's defaults when it has none).
    pub fn built_in_styles(&self) -> Vec<BuiltInStyle> {
        let Some(res) = self.doc.resolver.as_ref() else { return Vec::new() };
        let styles = &res.styles;
        let latent = &styles.latent;
        let category = |k: StyleKind| match k {
            StyleKind::Paragraph => StyleCategory::Paragraph,
            StyleKind::Character => StyleCategory::Character,
            StyleKind::Table => StyleCategory::Table,
            StyleKind::Numbering => StyleCategory::List,
        };
        // The exceptions by name, once: a document may list any number.
        let exceptions: std::collections::HashMap<String, &crate::docx::styles::LatentStyle> =
            latent.exceptions.iter().map(|e| (e.name.to_ascii_lowercase(), e)).collect();
        let visibility = |semi: bool, unhide: bool| match (semi, unhide) {
            (true, true) => StyleVisibility::HideUntilUsed,
            (true, false) => StyleVisibility::Hide,
            (false, _) => StyleVisibility::Show,
        };
        builtin::BUILT_IN
            .iter()
            .map(|b| {
                if let Some(s) = styles.get(b.id) {
                    return BuiltInStyle {
                        id: StyleId(b.id.to_string()),
                        name: describe::display_name(s),
                        category: StyleCategory::of(s),
                        defined: true,
                        priority: s.priority.unwrap_or(99),
                        visibility: visibility(s.meta.semi_hidden || s.meta.hidden, s.meta.unhide_when_used),
                        quick: s.quick,
                        locked: s.meta.locked,
                    };
                }
                let name = if b.name.chars().any(char::is_uppercase) {
                    b.name.to_string()
                } else {
                    b.name
                        .split(' ')
                        .map(|w| w.chars().take(1).flat_map(char::to_uppercase).chain(w.chars().skip(1)).collect::<String>())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                let (priority, semi, unhide, quick, locked) = match &latent.raw {
                    None => (b.priority, b.semi_hidden, b.unhide_when_used, b.quick, false),
                    Some(_) => {
                        let e = exceptions.get(&b.name.to_ascii_lowercase()).copied();
                        (
                            e.and_then(|e| e.priority).or(latent.priority).unwrap_or(99),
                            e.and_then(|e| e.semi_hidden).unwrap_or(latent.semi_hidden),
                            e.and_then(|e| e.unhide_when_used).unwrap_or(latent.unhide_when_used),
                            e.and_then(|e| e.quick).unwrap_or(latent.quick),
                            e.and_then(|e| e.locked).unwrap_or(latent.locked),
                        )
                    }
                };
                BuiltInStyle {
                    id: StyleId(b.id.to_string()),
                    name,
                    category: category(b.kind),
                    defined: false,
                    priority,
                    visibility: visibility(semi, unhide),
                    quick,
                    locked,
                }
            })
            .collect()
    }

    /// Sets the list flags of a built-in style the document doesn't define: its latent style exception, made when
    /// the document has none (with Word's exceptions for every built-in style, so the others keep theirs).
    pub(super) fn set_latent_meta(&mut self, m: &crate::api::command::StyleMetaChange, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let Some(b) = builtin::BUILT_IN.iter().find(|b| b.id == m.id.0) else { return Outcome::Refused(Error::unavailable("no such style")) };
        let cx = res.styles.cx();
        let w = cx.w.clone();
        let raw = res.styles.latent.raw.clone().unwrap_or_else(|| {
            let exceptions: String = builtin::BUILT_IN
                .iter()
                .map(|b| {
                    let mut x = format!("<{w}lsdException {w}name=\"{}\"", esc(b.name));
                    for (on, a) in [(b.semi_hidden, "semiHidden"), (b.unhide_when_used, "unhideWhenUsed")] {
                        if on {
                            x.push_str(&format!(" {w}{a}=\"1\""));
                        }
                    }
                    if b.priority != 99 {
                        x.push_str(&format!(" {w}uiPriority=\"{}\"", b.priority));
                    }
                    if b.quick {
                        x.push_str(&format!(" {w}qFormat=\"1\""));
                    }
                    x + "/>"
                })
                .collect();
            format!(
                "<{w}latentStyles {w}defLockedState=\"0\" {w}defUIPriority=\"99\" {w}defSemiHidden=\"0\" {w}defUnhideWhenUsed=\"0\" {w}defQFormat=\"0\" {w}count=\"{}\">{exceptions}</{w}latentStyles>",
                builtin::BUILT_IN.len()
            )
        });
        let mut el = Props::parse(Some(&raw), "latentStyles", &cx);
        let named = |x: &str| cx.parse(x, |_, n| crate::docx::xml::attr(n, "name").is_some_and(|v| v.eq_ignore_ascii_case(b.name))).unwrap_or(false);
        let at = el.kids.iter().position(|(n, x)| n == "lsdException" && named(x));
        let was = at.map_or_else(|| format!("<{w}lsdException {w}name=\"{}\"/>", esc(b.name)), |i| el.kids[i].1.clone());
        let mut attrs: Vec<(String, String)> =
            cx.parse(&was, |_, n| n.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect::<Vec<_>>()).unwrap_or_default();
        let mut put = |name: &str, v: Option<String>| {
            attrs.retain(|(a, _)| a != name);
            if let Some(v) = v {
                attrs.push((name.to_string(), v));
            }
        };
        if let Some(v) = m.visibility {
            let (semi, unhide) = match v {
                StyleVisibility::Show => (false, false),
                StyleVisibility::HideUntilUsed => (true, true),
                StyleVisibility::Hide => (true, false),
            };
            put("semiHidden", Some(if semi { "1" } else { "0" }.into()));
            put("unhideWhenUsed", Some(if unhide { "1" } else { "0" }.into()));
        }
        if let Some(p) = m.priority {
            put("uiPriority", Some(p.to_string()));
        }
        if let Some(q) = m.quick {
            put("qFormat", Some(if q { "1" } else { "0" }.into()));
        }
        if let Some(l) = m.locked {
            put("locked", Some(if l { "1" } else { "0" }.into()));
        }
        let order = ["name", "locked", "uiPriority", "semiHidden", "unhideWhenUsed", "qFormat"];
        attrs.sort_by_key(|(a, _)| order.iter().position(|o| *o == a.as_str()).unwrap_or(order.len()));
        let x = format!("<{w}lsdException{}/>", attrs.iter().map(|(a, v)| format!(" {w}{a}=\"{}\"", esc(v))).collect::<String>());
        match at {
            Some(i) => el.kids[i].1 = x,
            None => el.add("lsdException", x),
        }
        let Some(now) = el.xml() else { return Outcome::Unchanged("no latent styles") };
        if res.styles.latent.raw.as_deref() == Some(now.as_str()) {
            return Outcome::Unchanged("already so");
        }
        self.edit_styles(
            "Style Options",
            |s| {
                s.set_latent_xml(now)?;
                Ok(Vec::new())
            },
            fonts,
        )
    }
}
