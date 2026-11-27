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
use crate::document::format::{Jc, ShadingPattern, Width};
use crate::docx::char_props::to_run_props;
use crate::docx::para_props::to_para_props;
use crate::docx::props::{ParaProps as XmlParaProps, RunProps, toggle};
use crate::docx::style_xml::{LatentDef, StyleDef};
use crate::font::Fonts;
use crate::format::{
    CellProp, CharProps, LatentStyle, LatentStyles, ListChoice, MarginSide, ParaProps, RegionKind, RowProp, Shading, Style, StyleFormat, StyleKind,
    StyleProp, StyleSet, StyleSheet, TableProp, TableSide, TextBorder,
};
use crate::services::styles::{NameRefusal, builtin, check_name, describe};

use super::Editor;
use super::commands::Outcome;
use super::styles::{edit_cells, edit_paras, edit_rows, edit_style, edit_tables, set_flag, set_names, style_holder, written_alike};
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
pub(super) fn own_paragraph(p: &XmlParaProps, all: &ParagraphFormatting) -> ParagraphFormatting {
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

/// A style id from a name: its ASCII letters and digits, numbered past the ids taken.
pub(super) fn id_for(name: &str, styles: &StyleSheet) -> String {
    let base: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
    let base = if base.is_empty() { "Style".to_string() } else { base };
    let free = |id: &String| !styles.styles.contains_key(id.as_str()) && builtin::BUILT_IN.iter().all(|b| b.id != id);
    (0..).map(|n| if n == 0 { base.clone() } else { format!("{base}{n}") }).find(free).unwrap_or(base)
}

/// The cell margin sides a command sets, by side.
fn margin_sides(m: &CellMargins) -> [(MarginSide, Option<Points>); 4] {
    [(MarginSide::Top, m.top), (MarginSide::Left, m.left), (MarginSide::Bottom, m.bottom), (MarginSide::Right, m.right)]
}

/// A table style edit by a table command, for the part `region` (none: the whole table).
pub(super) fn table_style_edit(e: &Editor, def: &StyleDef, region: Option<RegionKind>, c: &Command) -> Result<StyleDef, Error> {
    use super::table_format::{Edges, authored_border, twips};
    let whole_only = |what: &str| if region.is_some() { Err(Error::invalid(format!("{what} is the whole table's"))) } else { Ok(()) };
    let dxa = |p: Points| Some(Width::Dxa(twips(p.0)));
    Ok(match c {
        Command::SetCellShading(fill) => {
            let fill = fill.as_ref().map(|c| e.word_colour(c)).transpose().map_err(Error::invalid)?;
            let shading = fill.map(|c| Shading {
                pattern: ShadingPattern::Clear,
                color: Some(crate::format::Color::Auto),
                fill: Some(super::format::colour(c)),
            });
            edit_style(def, |s, set| {
                edit_cells(s, set, region, |e| {
                    e.shading = shading;
                    e.whole(CellProp::Shading);
                })
            })
        }
        Command::SetCellAlignment(a) => {
            let v = super::table_format::cell_alignment(*a);
            edit_style(def, |s, set| {
                edit_cells(s, set, region, |e| {
                    e.v_align = Some(v);
                    e.whole(CellProp::VAlign);
                })
            })
        }
        Command::SetCellMargins(m) => edit_style(def, |s, set| match region {
            Some(_) => edit_cells(s, set, region, |e| {
                for (side, v) in margin_sides(m) {
                    if let Some(v) = v {
                        e.margin(side, dxa(v));
                    }
                }
            }),
            None => edit_tables(s, set, None, |e| {
                for (side, v) in margin_sides(m) {
                    if let Some(v) = v {
                        e.margin(side, dxa(v));
                    }
                }
            }),
        }),
        // A part's own setting, over the whole table's; the whole table's said either way, as a base style's
        // setting would show through its absence.
        Command::SetRowCanBreak(on) => edit_style(def, |s, set| {
            edit_rows(s, set, region, |e| {
                e.cant_split = Some(!*on);
                e.whole(RowProp::CantSplit);
            })
        }),
        Command::SetTableAlignment(a) => {
            whole_only("the alignment")?;
            let jc = match a {
                HorizontalAlignment::Left => Jc::Left,
                HorizontalAlignment::Center => Jc::Center,
                HorizontalAlignment::Right => Jc::Right,
            };
            edit_style(def, |s, set| {
                edit_tables(s, set, None, |e| {
                    e.align = Some(jc);
                    e.whole(TableProp::Align);
                })
            })
        }
        Command::SetTableIndent(i) => {
            whole_only("the indent")?;
            edit_style(def, |s, set| {
                edit_tables(s, set, None, |e| {
                    e.indent = dxa(*i);
                    e.whole(TableProp::Indent);
                })
            })
        }
        Command::SetCellBorders(b) => {
            let line = b.border.as_ref().map(|l| super::readback::BorderLine::of_api(l, 0.0, false));
            let (edges, line) = e.border_lines(b.edges, line.as_ref()).map_err(Error::invalid)?;
            let nil = Edges::None == edges;
            use TableSide as S;
            let sides: &[TableSide] = match edges {
                Edges::All | Edges::None => &[S::Top, S::Left, S::Bottom, S::Right, S::InsideH, S::InsideV],
                Edges::Outside => &[S::Top, S::Left, S::Bottom, S::Right],
                Edges::Inside => &[S::InsideH, S::InsideV],
                Edges::InsideH => &[S::InsideH],
                Edges::InsideV => &[S::InsideV],
                Edges::Top => &[S::Top],
                Edges::Bottom => &[S::Bottom],
                Edges::Left => &[S::Left],
                Edges::Right => &[S::Right],
                Edges::DiagonalDown | Edges::DiagonalUp => &[],
            };
            let diagonals: &[TableSide] = match edges {
                Edges::DiagonalDown => &[S::Tl2br],
                Edges::DiagonalUp => &[S::Tr2bl],
                Edges::None => &[S::Tl2br, S::Tr2bl],
                _ => &[],
            };
            let border = if nil {
                TextBorder { style: crate::docx::props::BorderStyle::Nil, width: 0.0, space: 0.0, shadow: false, color: None }
            } else {
                authored_border(&line)
            };
            edit_style(def, |s, set| match region {
                Some(_) => edit_cells(s, set, region, |e| {
                    for side in sides.iter().chain(diagonals) {
                        e.border(*side, Some(border));
                    }
                }),
                None => {
                    if !sides.is_empty() {
                        edit_tables(s, set, None, |e| {
                            for side in sides {
                                e.border(*side, Some(border));
                            }
                        });
                    }
                    if !diagonals.is_empty() {
                        edit_cells(s, set, None, |e| {
                            for side in diagonals {
                                e.border(*side, Some(border));
                            }
                        });
                    }
                }
            })
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
type Written = Vec<StyleDef>;

/// A new style of `kind` with id `id` and names `names`, not built in, in the quick styles gallery.
pub(super) fn new_style(id: &str, kind: StyleKind, names: &str) -> Style {
    let mut s = Style { id: id.into(), kind, custom: Some(true), ..Style::default() };
    set_names(&mut s, &mut Vec::new(), names);
    s
}

/// A style made from formatting: of `kind` with id `id` and names `name`, based on `base`, a paragraph style followed
/// by itself, in the quick styles gallery, with the formatting `ppr` and `rpr`.
pub(super) fn created_style(id: &str, kind: StyleKind, name: &str, base: Option<&str>, ppr: Option<ParaProps>, rpr: Option<CharProps>) -> Style {
    let mut s = new_style(id, kind, name);
    s.based_on = base.map(Rc::from);
    s.next = (kind == StyleKind::Paragraph).then(|| Rc::from(id));
    s.quick = Some(true);
    (s.format.para, s.format.char) = (ppr.unwrap_or_default(), rpr.unwrap_or_default());
    s
}

/// The character half of linked paragraph style `id` (named `name`): `char_id`, its character formatting `char`.
pub(super) fn char_half(char_id: &str, name: &str, id: &str, char: CharProps) -> Style {
    let mut s = new_style(char_id, StyleKind::Character, &format!("{name} Char"));
    s.based_on = Some("DefaultParagraphFont".into());
    s.link = Some(id.into());
    s.format.char = char;
    s
}

/// `def` with its link set to `link`, noted.
fn linked(def: &StyleDef, link: Option<&str>) -> StyleDef {
    edit_style(def, |s, set| {
        s.link = link.map(Rc::from);
        set.push(StyleSet::Style(StyleProp::Link));
    })
}

impl Editor {
    /// The formatting state of text in a style's XML, as the API carries a style being edited: what a style
    /// definition reads.
    pub fn style_formatting_state(&self, raw: &str) -> Option<(CharacterFormatting, ParagraphFormatting, Vec<crate::api::services::TabStopInfo>)> {
        self.formatting_state_of(&self.style_of_xml(raw)?)
    }

    /// The formatting state of text in a style: what a style definition reads.
    pub(super) fn formatting_state_of(
        &self,
        def: &StyleDef,
    ) -> Option<(CharacterFormatting, ParagraphFormatting, Vec<crate::api::services::TabStopInfo>)> {
        let (para, run, _) = self.style_text(def)?;
        let tabs: Vec<_> =
            para.style.tabs.iter().filter(|t| t.kind != crate::docx::props::TabKind::Clear).map(|t| super::commands::tabs::info(t, false)).collect();
        // Its list, its own or a base style's: bulleted or numbered
        let res = self.doc.resolver.as_ref()?;
        let list = para.style.num.filter(|ListRef { list: ListId(n), level: _ }| *n != 0).and_then(|ListRef { list, level }| {
            res.cascade().level(list, level).map(|l| crate::docx::list_xml::format_name(l.format.as_ref()) == "bullet")
        });
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

    /// The document's style `id` and its holder, or the built-in style it names when the document doesn't define it
    /// (`false`).
    fn style_or_built_in(&self, id: &str) -> Option<(Rc<Style>, StyleDef, bool)> {
        let typed = self.doc.resolver.as_ref()?.typed();
        if let (Some(s), Some(d)) = (typed.cascade.styles.get(id), style_holder(&typed, id)) {
            return Some((s.clone(), d, true));
        }
        let b = builtin::BUILT_IN.iter().find(|b| b.id == id)?;
        let s = Rc::new(builtin::definition(b));
        (!typed.styles.loops(&s)).then(|| (s.clone(), StyleDef::typed(s), false))
    }

    /// A style of kind `kind` a change may refer to: the document's, or a built-in one it adds (`Some`).
    fn referable(&self, id: &StyleId, kind: StyleKind, what: &str) -> Result<Option<StyleDef>, Error> {
        let typed = self.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?.typed();
        match typed.cascade.styles.get(&id.0) {
            Some(s) if s.kind == kind => Ok(None),
            Some(_) => Err(Error::invalid(format!("the {what} is a style of another type"))),
            None => match builtin::BUILT_IN.iter().find(|b| b.id == id.0 && b.kind == kind) {
                Some(b) => Ok(Some(StyleDef::typed(Rc::new(builtin::definition(b))))),
                None => Err(Error::invalid(format!("the {what} doesn't exist"))),
            },
        }
    }

    /// Runs formatting commands on a style (the part `region` of a table style, none: its own), in order: the
    /// character and paragraph ones as on text, the table ones on its table properties.
    fn format_style_with(
        &mut self,
        def: StyleDef,
        kind: StyleKind,
        region: Option<RegionKind>,
        commands: &[Command],
        fonts: &Fonts,
    ) -> Result<StyleDef, Error> {
        self.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?;
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
        let mut def = def;
        let mut k = 0;
        while k < commands.len() {
            if table_command(&commands[k]) {
                def = table_style_edit(self, &def, region, &commands[k])?;
                k += 1;
                continue;
            }
            let end = commands[k..].iter().position(table_command).map_or(commands.len(), |e| k + e);
            self.style_part = region;
            let out = self.format_style(&def, commands[k..end].to_vec(), fonts);
            self.style_part = None;
            def = out?;
            k = end;
        }
        Ok(def)
    }

    /// A style with `change` made to it: `def` is the style as it is (or as made), `kind` its kind. Also answers the
    /// other styles to write with it.
    pub(super) fn changed_style(
        &mut self,
        def: StyleDef,
        kind: StyleKind,
        change: &StyleChange,
        fonts: &Fonts,
    ) -> Result<(StyleDef, Written), Error> {
        let typed = self.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?.typed();
        let (part, styles) = (&*typed.styles, &*typed.cascade.styles);
        let holder = |id: &str| style_holder(&typed, id).ok_or_else(|| Error::invalid("not a style"));
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
        let mut def = def;
        if change.clear {
            // Its own formatting but its list
            def = edit_style(&def, |s, _| {
                let p = &s.format.para;
                let list = |n: &crate::format::QName| &*n.ns == crate::docx::xml::W && &*n.local == "numPr";
                let foreign = p.foreign.as_deref().map(|f| crate::format::ForeignProps {
                    children: Vec::new(),
                    attributes: f.attributes.iter().filter(|a| a.on.as_ref().is_some_and(list)).cloned().collect(),
                    kept: f.kept.iter().filter(|c| list(&c.name)).cloned().collect(),
                });
                let foreign = foreign.filter(|f| *f != crate::format::ForeignProps::default()).map(Rc::new);
                s.format.para = ParaProps { list: p.list, list_level: p.list_level, foreign, ..ParaProps::default() };
                s.format.char = CharProps::default();
                if kind == StyleKind::Table {
                    (s.format.table, s.format.row, s.format.cell) = Default::default();
                }
            });
        }
        def = self.format_style_with(def, kind, None, &change.formatting, fonts)?;
        for p in &change.parts {
            let region = p.part.region().and_then(|r| crate::docx::style_xml::region_kind(Some(r)));
            if p.clear {
                def = edit_style(&def, |s, set| match region {
                    Some(r) => {
                        s.regions.retain(|x| x.kind != r);
                        set.push(StyleSet::Region(r));
                    }
                    None => s.format = StyleFormat::default(),
                });
            }
            def = self.format_style_with(def, kind, region, &p.formatting, fonts)?;
        }
        if change.row_band.is_some() || change.column_band.is_some() {
            for (n, prop) in [(change.row_band, TableProp::RowBand), (change.column_band, TableProp::ColBand)] {
                let Some(n) = n else { continue };
                super::commands::bounds::within("the band size", n as f64, 1.0, 32767.0)?;
                def = edit_style(&def, |s, set| {
                    edit_tables(s, set, None, |e| {
                        match prop {
                            TableProp::RowBand => e.row_band = Some(i64::from(n)),
                            _ => e.col_band = Some(i64::from(n)),
                        }
                        e.whole(prop);
                    })
                });
            }
        }
        if part.loops(def.get()) {
            return Err(Error::invalid("not a style"));
        }
        let id = def.get().id.to_string();
        if let Some(b) = &change.based_on {
            if let Some(b) = b {
                if b.0 == id {
                    return Err(Error::invalid("a style can't be based on itself"));
                }
                also.extend(self.referable(b, kind, "base")?);
            }
            def = edit_style(&def, |s, set| {
                s.based_on = b.as_ref().map(|b| Rc::from(b.0.as_str()));
                set.push(StyleSet::Style(StyleProp::BasedOn));
            });
        }
        if let Some(n) = &change.next {
            if let Some(n) = n.as_ref().filter(|n| n.0 != id) {
                also.extend(self.referable(n, StyleKind::Paragraph, "following style")?);
            }
            def = edit_style(&def, |s, set| {
                s.next = n.as_ref().map(|n| Rc::from(n.0.as_str()));
                set.push(StyleSet::Style(StyleProp::Next));
            });
        }
        if let Some(on) = change.auto_update {
            def = edit_style(&def, |s, set| set_flag(s, set, StyleProp::AutoRedefine, on));
        }
        if let Some(on) = change.quick {
            def = edit_style(&def, |s, set| set_flag(s, set, StyleProp::Quick, on));
        }
        if let Some(l) = &change.list {
            if let Some(l) = l {
                if !typed.cascade.lists.lists.contains_key(&ListId(l.list)) || l.list == 0 {
                    return Err(Error::invalid("no such list"));
                }
                if l.level > 8 {
                    return Err(Error::invalid("a list level is 0 to 8"));
                }
            }
            def = edit_style(&def, |s, set| {
                edit_paras(s, set, None, |e| {
                    // No list over a base style's: none.
                    (e.list, e.list_level) = match l {
                        Some(StyleList { list, level: 0 }) => (Some(ListChoice::In(ListId(*list))), None),
                        Some(StyleList { list, level }) => (Some(ListChoice::In(ListId(*list))), Some(i64::from(*level))),
                        None => (Some(ListChoice::None), None),
                    };
                    e.whole(crate::format::ParaProp::List);
                })
            });
        }
        if let Some(name) = &change.name {
            let first = name.trim();
            if part.loops(def.get()) {
                return Err(Error::invalid("not a style"));
            }
            let cur = def.get().clone();
            if !describe::display_name(&cur).eq_ignore_ascii_case(first) {
                check_name(styles, first, Some(&id)).map_err(name_error)?;
                // A built-in style keeps its name: the new one is an alias.
                let aliases = cur.aliases.iter().map(|a| a.to_string());
                let names: Vec<String> = if cur.custom.unwrap_or(false) {
                    std::iter::once(first.to_string()).chain(aliases).collect()
                } else {
                    [cur.name.as_deref().unwrap_or_default().to_string(), first.to_string()].into_iter().chain(aliases).collect()
                };
                def = edit_style(&def, |s, set| set_names(s, set, &names.join(",")));
            }
        }
        // Links: this style and its new partner name each other; their old partners name neither.
        let link_of = |id: &str| styles.get(id).and_then(|s| s.link.as_deref().map(str::to_string));
        let was_link = link_of(&id);
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
            def = linked(&def, link.as_deref());
            if let Some(old) = was_link.as_ref().filter(|o| Some(*o) != link.as_ref())
                && styles.get(old).is_some_and(|s| s.link.as_deref() == Some(&*id))
            {
                also.push(linked(&holder(old)?, None));
            }
            if let Some(new) = &link
                && let Some(s) = styles.get(new)
                && s.link.as_deref() != Some(&*id)
            {
                if let Some(partner) = s.link.as_deref().filter(|p| **p != *id)
                    && styles.get(partner).is_some_and(|p| p.link.as_deref() == Some(new.as_str()))
                {
                    also.push(linked(&holder(partner)?, None));
                }
                also.push(linked(&holder(new)?, Some(&id)));
            }
        }
        // A linked style's other half takes its character formatting.
        let before = styles.get(&id).map_or_else(CharProps::default, |s| s.format.char.clone());
        if let Some(l) = &link
            && before != def.get().format.char
            && matches!(kind, StyleKind::Paragraph | StyleKind::Character)
        {
            let half = also.iter().position(|d| *d.get().id == **l);
            let base = match half {
                Some(i) => Some(also.remove(i)),
                None => styles.get(l).filter(|s| s.link.as_deref() == Some(&*id)).map(|_| holder(l)).transpose()?,
            };
            if let Some(base) = base {
                let char = def.get().format.char.clone();
                also.push(edit_style(&base, |s, _| s.format.char = char));
            }
        }
        Ok((def, also))
    }

    /// Writes a style and the styles with it as one undo step, then a list style's levels in the same step.
    fn write_style(&mut self, label: &'static str, def: StyleDef, also: Written, kind: StyleKind, change: &StyleChange, fonts: &Fonts) -> Outcome {
        let typed = self.doc.resolver.as_ref().map(|r| r.typed());
        let id = def.get().id.to_string();
        let relabel = change.list.is_some();
        let levels = change.list_levels.clone();
        // Styles it adds go first (a built-in base is there when it is read), then it, then those it changes.
        let (fresh, rest): (Written, Written) = also
            .into_iter()
            .partition(|d| typed.as_ref().is_some_and(|t| !t.styles.loops(d.get()) && !t.cascade.styles.styles.contains_key(&d.get().id)));
        self.joined(label, fonts, |e| {
            let out = e.edit_styles_relabel(
                label,
                |s| {
                    let mut changed = Vec::new();
                    for d in fresh {
                        changed.extend(s.set(d)?);
                    }
                    changed.extend(s.set(def)?);
                    for d in rest {
                        changed.extend(s.set(d)?);
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
        let Some((style, was, defined)) = self.style_or_built_in(&id.0) else { return Outcome::Refused(Error::unavailable("no such style")) };
        if let Some(o) = self.refuse_style_edit() {
            return o;
        }
        let (def, also) = match self.changed_style(was.clone(), style.kind, &change, fonts) {
            Ok(x) => x,
            Err(e) => return Outcome::Refused(e),
        };
        let same = self.doc.resolver.as_ref().is_some_and(|r| written_alike(&r.typed(), &def, &was));
        if defined && same && also.is_empty() && change.list_levels.is_none() {
            return Outcome::Unchanged("the style already is so");
        }
        self.write_style("Modify Style", def, also, style.kind, &change, fonts)
    }

    /// Makes a style, as one undo step (applied too, when asked); its id, made from its name.
    pub fn create_style(&mut self, new: NewStyle, fonts: &Fonts) -> (Outcome, Option<StyleId>) {
        if let Some(o) = self.refuse_style_edit() {
            return (o, None);
        }
        let Some(res) = self.doc.resolver.clone() else { return (Outcome::Unchanged("no styles"), None) };
        let typed = res.typed();
        let (part, styles) = (&*typed.styles, &*typed.cascade.styles);
        let name = match &new.change.name {
            Some(n) => n.trim().to_string(),
            None => crate::services::styles::next_new_name(styles),
        };
        if let Err(e) = check_name(styles, &name, None) {
            return (Outcome::Refused(name_error(e)), None);
        }
        let kind = new.category.kind();
        let id = id_for(&name, styles);
        // Based on the selection's paragraph style (a paragraph style), or the default one of its type.
        let para = self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para(&self.doc, p));
        let base = match kind {
            StyleKind::Paragraph if new.from_selection => {
                para.and_then(|p| p.style.style_id.clone()).or_else(|| styles.default_paragraph.as_deref().map(str::to_string))
            }
            StyleKind::Paragraph => styles.default_paragraph.as_deref().map(str::to_string),
            StyleKind::Character => {
                Some("DefaultParagraphFont".to_string()).filter(|d| styles.get(d).is_some_and(|s| s.kind == StyleKind::Character))
            }
            StyleKind::Table => styles.default_table.as_deref().map(str::to_string),
            StyleKind::Numbering => None,
        };
        let (ppr, rpr) = if new.from_selection { self.selection_formatting() } else { (None, None) };
        let (ppr, rpr) = match kind {
            StyleKind::Paragraph => (ppr, rpr),
            StyleKind::Character => (None, rpr),
            _ => (None, None),
        };
        let mut style = created_style(&id, kind, &name, base.as_deref(), ppr, rpr);
        if part.loops(&style) {
            return (Outcome::Refused(Error::invalid("not a style")), None);
        }
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
                .find(|c| !styles.styles.contains_key(c.as_str()))
                .unwrap_or_default();
            style.link = Some(char_id.as_str().into());
            half = Some(char_id);
        }
        let def = StyleDef::typed(Rc::new(style));
        let label = "New Style";
        let apply = new.apply;
        let category = new.category;
        let out = self.joined(label, fonts, |e| {
            // The style alone first, so the change refers to it as it is.
            let out = e.edit_styles(label, |s| s.set(def.clone()), fonts);
            if matches!(out, Outcome::Refused(_)) {
                return out;
            }
            let (def, mut also) = match e.changed_style(def.clone(), kind, &change, fonts) {
                Ok(x) => x,
                Err(err) => return Outcome::Refused(err),
            };
            if let Some(c) = &half {
                also.push(StyleDef::typed(Rc::new(char_half(c, &name, &id, def.get().format.char.clone()))));
            }
            let written = e.write_style(label, def, also, kind, &change, fonts);
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
        let made = self.doc.resolver.as_ref().is_some_and(|r| r.cascade().styles.styles.contains_key(id.as_str()));
        (out, made.then(|| StyleId(id.clone())))
    }

    /// Style `id` in full, without a dialog: its names, type, relations, flags, list, and formatting resolved and as
    /// set by it, with a table style's parts and a list style's levels. A built-in style the document doesn't define
    /// reads as it would be once used.
    pub fn style_definition(&self, id: &StyleId) -> Option<StyleDefinition> {
        let res = self.doc.resolver.as_ref()?;
        let (_, def, defined) = self.style_or_built_in(&id.0)?;
        let next = if defined {
            res.clone()
        } else {
            let mut next = (**res).clone();
            next.set_style(def.clone()).ok()?;
            Rc::new(next)
        };
        let c = next.cascade();
        let e = c.styles.styles.get(id.0.as_str())?;
        let style = &*e.style;
        let colours = &c.colours;
        let format = c.format_of(e);
        let (character, paragraph, tabs) = match style.kind {
            StyleKind::Table => {
                let region = c.table_region(Some(&style.id), &[]);
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
            _ => self.formatting_state_of(&def)?,
        };
        let ppr = to_para_props(&format.para, colours);
        let own_character = own_character(&to_run_props(&format.char, colours), &character);
        let own_paragraph = if style.kind == StyleKind::Character { ParagraphFormatting::default() } else { own_paragraph(&ppr, &paragraph) };
        let list = ppr
            .num_id
            .filter(|n| *n != 0 && style.kind == StyleKind::Paragraph)
            .map(|list| StyleList { list, level: ppr.ilvl.unwrap_or(0).clamp(0, 8) as u8 });
        let list_levels = match style.kind {
            StyleKind::Numbering => match c.chain_para(Some(&style.id)).list {
                Some(crate::format::ListChoice::In(ListId(n))) if n != 0 => self.list_levels(n).unwrap_or_default(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        let table = (style.kind == StyleKind::Table).then(|| self.table_style_definition(&next, &style.id)).flatten();
        let on = |v: Option<bool>| v.unwrap_or(false);
        let text = |v: &Option<Rc<str>>| v.as_deref().map(str::to_string);
        Some(StyleDefinition {
            id: StyleId(style.id.to_string()),
            name: describe::display_name(style),
            aliases: style.aliases.iter().map(|a| a.to_string()).collect(),
            category: StyleCategory::of(style),
            built_in: !on(style.custom),
            defined,
            based_on: c.based_on(e).map(|b| StyleId(b.to_string())),
            next: text(&style.next).filter(|n| **n != *style.id).map(StyleId),
            link: text(&style.link).filter(|l| c.styles.styles.contains_key(l.as_str())).map(StyleId),
            auto_update: on(style.meta.auto_redefine),
            quick: on(style.quick),
            visibility: match (on(style.meta.semi_hidden) || on(style.meta.hidden), on(style.meta.unhide_when_used)) {
                (true, true) => StyleVisibility::HideUntilUsed,
                (true, false) => StyleVisibility::Hide,
                (false, _) => StyleVisibility::Show,
            },
            priority: style.priority.unwrap_or(99),
            locked: on(style.meta.locked),
            list,
            character,
            paragraph,
            tabs,
            own_character,
            own_paragraph,
            description: describe::describe(e, &c),
            table,
            list_levels,
        })
    }

    /// A table style's table formatting and its parts, read with `res` (which holds it).
    fn table_style_definition(&self, res: &crate::docx::resolvers::Resolvers, id: &str) -> Option<TableStyleDefinition> {
        use crate::docx::table_props::{tbl_props, tc_props, tr_props};
        let c = res.cascade();
        let colours = &c.colours;
        let whole = tbl_props(&c.table_region(Some(id), &[]).format.table, colours);
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
                let typed = c.table_region(Some(id), &crate::docx::style_xml::region_kinds(region));
                let (tc, tr) = (tc_props(&typed.format.cell, colours), tr_props(&typed.format.row));
                let para = crate::docx::para_xml::resolve(res, &Default::default(), None, Some(&typed));
                let run = Rc::new(run_xml::resolve(res, &para.base, Default::default(), false));
                let mut b = borders(&tc.borders);
                if region.is_none() {
                    b = TableBorders { diagonal_down: b.diagonal_down, diagonal_up: b.diagonal_up, ..borders(&whole.borders) };
                }
                let defines = |g: &str| crate::docx::style_xml::region_kind(Some(g)).is_some_and(|k| c.table_defines(Some(id), k));
                TablePartDefinition {
                    part,
                    defined: region.is_none_or(defines),
                    character: super::state::character(&[&run]),
                    paragraph: super::state::paragraph(&[(&para.style, None)]),
                    fill: tc.shading.flatten().map(Color),
                    cell_alignment: super::table_format::api_cell_alignment(tc.v_align),
                    borders: b,
                    cell_margins: if region.is_none() { margins(&whole.margins) } else { margins(&tc.margins) },
                    row_can_break: !tr.cant_split.unwrap_or(false),
                }
            })
            .collect();
        Some(TableStyleDefinition {
            alignment: match whole.jc {
                Some(crate::docx::props::Jc::Center) => HorizontalAlignment::Center,
                Some(crate::docx::props::Jc::Right) => HorizontalAlignment::Right,
                _ => HorizontalAlignment::Left,
            },
            indent: Points(whole.ind.map_or(0.0, crate::docx::props::Width::dxa)),
            fill: whole.shd.as_ref().and_then(crate::docx::props::Shd::flat).map(Color),
            row_band: whole.row_band.unwrap_or(1).clamp(1, u32::MAX as i64) as u32,
            column_band: whole.col_band.unwrap_or(1).clamp(1, u32::MAX as i64) as u32,
            parts,
        })
    }

    /// Every built-in style, with whether the document defines it and the flags style lists use: the document's own
    /// for those it defines, its latent styles' for the rest (Word's defaults when it has none).
    pub fn built_in_styles(&self) -> Vec<BuiltInStyle> {
        let Some(res) = self.doc.resolver.as_ref() else { return Vec::new() };
        let c = res.cascade();
        let styles = &*c.styles;
        let none = crate::format::LatentStyles::default();
        let latent = styles.latent.as_deref().unwrap_or(&none);
        let category = |k: StyleKind| match k {
            StyleKind::Paragraph => StyleCategory::Paragraph,
            StyleKind::Character => StyleCategory::Character,
            StyleKind::Table => StyleCategory::Table,
            StyleKind::Numbering => StyleCategory::List,
        };
        // The exceptions by name, once: a document may list any number.
        let exceptions: std::collections::HashMap<String, &crate::format::LatentStyle> =
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
                    let on = |v: Option<bool>| v.unwrap_or(false);
                    return BuiltInStyle {
                        id: StyleId(b.id.to_string()),
                        name: describe::display_name(s),
                        category: StyleCategory::of(s),
                        defined: true,
                        priority: s.priority.unwrap_or(99),
                        visibility: visibility(on(s.meta.semi_hidden) || on(s.meta.hidden), on(s.meta.unhide_when_used)),
                        quick: on(s.quick),
                        locked: on(s.meta.locked),
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
                let (priority, semi, unhide, quick, locked) = match &styles.latent {
                    None => (b.priority, b.semi_hidden, b.unhide_when_used, b.quick, false),
                    Some(_) => {
                        let e = exceptions.get(&b.name.to_ascii_lowercase()).copied();
                        let on = |v: Option<bool>| v.unwrap_or(false);
                        (
                            e.and_then(|e| e.priority).or(latent.priority).unwrap_or(99),
                            e.and_then(|e| e.semi_hidden).unwrap_or(on(latent.semi_hidden)),
                            e.and_then(|e| e.unhide_when_used).unwrap_or(on(latent.unhide_when_used)),
                            e.and_then(|e| e.quick).unwrap_or(on(latent.quick)),
                            e.and_then(|e| e.locked).unwrap_or(on(latent.locked)),
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
        let typed = res.typed();
        let held = typed.styles.latent.as_ref();
        let def = latent_edited(held, b, m);
        let (colours, cx) = (&typed.cascade.colours, typed.styles.cx());
        if held.is_some_and(|h| h.xml(colours, &cx) == def.xml(colours, &cx)) {
            return Outcome::Unchanged("already so");
        }
        self.edit_styles(
            "Style Options",
            |s| {
                s.set_latent(def)?;
                Ok(Vec::new())
            },
            fonts,
        )
    }
}

/// The latent styles `held` (none: Word's) with built-in style `b`'s exception given the list flags `m` sets, made
/// when there is none.
pub(super) fn latent_edited(held: Option<&LatentDef>, b: &builtin::BuiltIn, m: &crate::api::command::StyleMetaChange) -> LatentDef {
    let mut l = held.map_or_else(word_latent, |d| (**d.get()).clone());
    let at = l.exceptions.iter().position(|x| x.name.eq_ignore_ascii_case(b.name));
    let mut x = at.map_or_else(|| LatentStyle { name: b.name.into(), ..LatentStyle::default() }, |i| l.exceptions[i].clone());
    if let Some(v) = m.visibility {
        let (semi, unhide) = match v {
            StyleVisibility::Show => (false, false),
            StyleVisibility::HideUntilUsed => (true, true),
            StyleVisibility::Hide => (true, false),
        };
        (x.semi_hidden, x.unhide_when_used) = (Some(semi), Some(unhide));
    }
    if let Some(p) = m.priority {
        x.priority = Some(p);
    }
    if let Some(q) = m.quick {
        x.quick = Some(q);
    }
    if let Some(locked) = m.locked {
        x.locked = Some(locked);
    }
    match at {
        Some(i) => l.exceptions[i] = x,
        None => l.exceptions.push(x),
    }
    match held {
        Some(h) => h.edited(l, &[]),
        None => LatentDef::typed(Rc::new(l)),
    }
}

/// Word's latent styles for a document that has none: nothing locked or hidden, priority 99, with an exception for
/// every built-in style giving its own flags.
fn word_latent() -> LatentStyles {
    let on = |v: bool| v.then_some(true);
    let exceptions = builtin::BUILT_IN
        .iter()
        .map(|b| LatentStyle {
            name: b.name.into(),
            priority: (b.priority != 99).then_some(b.priority),
            semi_hidden: on(b.semi_hidden),
            unhide_when_used: on(b.unhide_when_used),
            quick: on(b.quick),
            ..LatentStyle::default()
        })
        .collect();
    LatentStyles {
        locked: Some(false),
        priority: Some(99),
        semi_hidden: Some(false),
        unhide_when_used: Some(false),
        quick: Some(false),
        count: Some(builtin::BUILT_IN.len() as u32),
        exceptions,
        foreign: None,
    }
}
