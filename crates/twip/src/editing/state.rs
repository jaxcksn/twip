//! Formatting and command state

use crate::docx::origin::FloatOrigin;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::api::catalog::catalog;
use crate::api::command::{
    Command, CommandId, MultiplePagesMode, NotePosition, NumberFormat, PageVerticalAlignment, ParagraphSpacing, SectionBreakKind, TextFlow,
};
use crate::api::state::{
    BorderState, CharacterFormatting, ColumnLayout, CommandState, CommandValue, ElementContext, ElementKind, FlowState, HeaderFooterInfo,
    HeaderFooterKind, HeaderFooterType, IndentState, LinkInfo, ListState, ObjectSelection, PageFormatting, PageMargins, PageSize,
    ParagraphFormatting, TableSelection, TableStyleOptions, Tri,
};
use crate::api::value::{
    Alignment, AutoOr, Color, HeightRule, HighlightColor, HorizontalAlignment, LineSpacing, NoneOr, ObjectWrap, Orientation, Points, RowHeight,
    StyleId, TableWidth, UnderlineStyle,
};
use crate::docx::props::{Jc, LineRule, VertAlign};
use crate::docx::{FloatWrap, ParaStyle, Run, RunStyle, Underline};
use crate::layout::{ParaId, TableId};

use super::Editor;
use super::keymap::{Bound, KeyContext, Mods, Toggle};
use super::position::StoryId;
use super::selection::{ObjectRef, Selection};
use crate::document::paragraph::{ListId, ListRef};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Picture,
    Float,
    TextBox,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectedObject {
    pub kind: ObjectKind,
    pub width: f64,
    pub height: f64,
    pub alt: Rc<str>,
    pub decorative: bool,
    pub wrap: Option<(FloatWrap, bool)>,
    pub locked: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct FormatState {
    pub run: Run,
    pub character: CharacterFormatting,
    pub paragraph: ParagraphFormatting,
    pub para: Rc<ParaStyle>,
    pub list: Option<bool>,
    pub has_selection: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub table: Option<TableId>,
    pub object: Option<ObjectKind>,
    pub object_size: Option<(f64, f64)>,
    pub object_alt: Option<Rc<str>>,
    pub cell: Option<(usize, usize)>,
    pub story: StoryId,
    pub section: usize,
}

pub(super) fn fold<T: PartialEq>(values: impl IntoIterator<Item = T>) -> Tri<T> {
    let mut out = Tri::Unavailable;
    for v in values {
        out = match out {
            Tri::Unavailable => Tri::Uniform(v),
            Tri::Uniform(u) if u == v => Tri::Uniform(u),
            _ => return Tri::Mixed,
        };
    }
    out
}

fn underline_of(r: &RunStyle) -> UnderlineStyle {
    underline_style(r.underline)
}

/// The underline style of a run's underline.
pub(super) fn underline_style(u: Option<Underline>) -> UnderlineStyle {
    match u {
        None | Some(Underline::None) => UnderlineStyle::None,
        Some(Underline::Single) => UnderlineStyle::Single,
        Some(Underline::Words) => UnderlineStyle::Words,
        Some(Underline::Double) => UnderlineStyle::Double,
        Some(Underline::Thick) => UnderlineStyle::Thick,
        Some(Underline::Dotted) => UnderlineStyle::Dotted,
        Some(Underline::Dash) => UnderlineStyle::Dash,
        Some(Underline::DotDash) => UnderlineStyle::DotDash,
        Some(Underline::Wave) => UnderlineStyle::Wave,
        Some(Underline::DottedHeavy) => UnderlineStyle::DottedHeavy,
        Some(Underline::DashedHeavy) => UnderlineStyle::DashedHeavy,
        Some(Underline::DashLong) => UnderlineStyle::DashLong,
        Some(Underline::DashLongHeavy) => UnderlineStyle::DashLongHeavy,
        Some(Underline::DashDotHeavy) => UnderlineStyle::DashDotHeavy,
        Some(Underline::DotDotDash) => UnderlineStyle::DotDotDash,
        Some(Underline::DashDotDotHeavy) => UnderlineStyle::DashDotDotHeavy,
        Some(Underline::WavyHeavy) => UnderlineStyle::WavyHeavy,
        Some(Underline::WavyDouble) => UnderlineStyle::WavyDouble,
    }
}

const HIGHLIGHTS: [HighlightColor; 15] = [
    HighlightColor::Yellow,
    HighlightColor::BrightGreen,
    HighlightColor::Turquoise,
    HighlightColor::Pink,
    HighlightColor::Blue,
    HighlightColor::Red,
    HighlightColor::DarkBlue,
    HighlightColor::Teal,
    HighlightColor::Green,
    HighlightColor::Violet,
    HighlightColor::DarkRed,
    HighlightColor::DarkYellow,
    HighlightColor::Gray50,
    HighlightColor::Gray25,
    HighlightColor::Black,
];

fn highlight_of(r: &RunStyle) -> NoneOr<HighlightColor> {
    highlight_color(r.highlight)
}

/// The highlight colour of a highlight's RGB.
pub(super) fn highlight_color(rgb: Option<u32>) -> NoneOr<HighlightColor> {
    let named = |rgb: u32| HIGHLIGHTS.into_iter().find(|h| crate::docx::props::highlight(super::commands::highlight_value(*h)) == Some(rgb));
    match rgb.and_then(named) {
        Some(h) => NoneOr::Value(h),
        None => NoneOr::None,
    }
}

/// The character formatting of `runs` together: each value uniform, mixed or unavailable.
pub fn character(runs: &[&RunStyle]) -> CharacterFormatting {
    let tri = |f: &dyn Fn(&RunStyle) -> bool| fold(runs.iter().map(|r| f(r)));
    let mut out = CharacterFormatting {
        font: fold(runs.iter().map(|r| r.ascii.clone())),
        size: fold(runs.iter().map(|r| Points(r.size))),
        bold: tri(&|r| r.bold),
        italic: tri(&|r| r.italic),
        underline: fold(runs.iter().map(|r| underline_of(r))),
        strikethrough: tri(&|r| r.strike),
        double_strikethrough: tri(&|r| r.dstrike),
        subscript: tri(&|r| r.vert_align == VertAlign::Subscript),
        superscript: tri(&|r| r.vert_align == VertAlign::Superscript),
        small_caps: tri(&|r| r.small_caps),
        all_caps: tri(&|r| r.caps),
        hidden: tri(&|r| r.hidden),
        color: fold(runs.iter().map(|r| r.color.map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c))))),
        highlight: fold(runs.iter().map(|r| highlight_of(r))),
        character_spacing: fold(runs.iter().map(|r| Points(r.spacing))),
        position: fold(runs.iter().map(|r| Points(r.position))),
        character_style: fold(runs.iter().map(|r| r.style_id.clone().map_or(NoneOr::None, |s| NoneOr::Value(StyleId(s))))),
        scale: fold(runs.iter().map(|r| r.scale)),
        kerning: fold(runs.iter().map(|r| Points(r.kern))),
        ligatures: {
            use crate::api::command::Ligatures as L;
            let l = |r: &RunStyle| match r.typography.ligatures {
                0 => Some(L::None),
                1 => Some(L::Standard),
                3 => Some(L::StandardContextual),
                12 => Some(L::HistoricalDiscretional),
                15 => Some(L::All),
                _ => None,
            };
            if runs.iter().any(|r| l(r).is_none()) {
                if runs.is_empty() { Tri::Unavailable } else { Tri::Mixed }
            } else {
                fold(runs.iter().filter_map(|r| l(r)))
            }
        },
        number_spacing: fold(runs.iter().map(|r| {
            use crate::api::command::NumberSpacing as N;
            match r.typography.number_spacing {
                4 => N::Proportional,
                8 => N::Tabular,
                _ => N::Default,
            }
        })),
        number_form: fold(runs.iter().map(|r| {
            use crate::api::command::NumberForm as N;
            match r.typography.number_form {
                1 => N::Lining,
                2 => N::OldStyle,
                _ => N::Default,
            }
        })),
        stylistic_set: fold(runs.iter().map(|r| {
            let s = r.typography.stylistic_sets;
            if s == 0 { 0 } else { s.trailing_zeros() as u8 + 1 }
        })),
        contextual_alternates: tri(&|r| r.typography.contextual_alternates),
        language: fold(runs.iter().map(|r| r.lang.clone())),
        no_proof: tri(&|r| r.no_proof),
        emboss: tri(&|r| r.emboss),
        engrave: tri(&|r| r.imprint),
        outline: tri(&|r| r.outline),
        shadow: tri(&|r| r.shadow),
        underline_color: fold(runs.iter().map(|r| r.underline_color.map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c))))),
        text_effects: fold(runs.iter().map(|r| r.effects.as_deref().map(crate::api::drawing::TextEffects::of).unwrap_or_default())),
        ..CharacterFormatting::default()
    };
    super::readback::character(runs).fill(&mut out);
    out
}

fn borders_of(p: &ParaStyle) -> BorderState {
    let on = |b: &Option<crate::docx::props::Border>| b.as_ref().is_some_and(|b| b.visible());
    match (on(&p.border_top), on(&p.border_left), on(&p.border_bottom), on(&p.border_right)) {
        (true, true, true, true) => BorderState::Box,
        (false, false, true, false) => BorderState::Bottom,
        (false, false, false, false) => BorderState::None,
        _ => BorderState::Other,
    }
}

/// The paragraphs' formatting; their tab stops are unavailable (the editor reads them).
pub fn paragraph(paras: &[(&ParaStyle, Option<bool>)]) -> ParagraphFormatting {
    let tri = |f: &dyn Fn(&ParaStyle) -> bool| fold(paras.iter().map(|(p, _)| f(p)));
    let pt = |f: &dyn Fn(&ParaStyle) -> f64| fold(paras.iter().map(|(p, _)| Points(f(p))));
    let mut out = ParagraphFormatting {
        style: fold(paras.iter().map(|(p, _)| StyleId(p.style_id.clone().unwrap_or_else(|| "Normal".into())))),
        alignment: fold(paras.iter().map(|(p, _)| match p.jc {
            Jc::Left => Alignment::Left,
            Jc::Center => Alignment::Center,
            Jc::Right => Alignment::Right,
            Jc::Both | Jc::Distribute => Alignment::Justify,
        })),
        line_spacing: fold(paras.iter().map(|(p, _)| match p.line_rule {
            LineRule::Auto => LineSpacing::Multiple { lines: p.line },
            LineRule::Exact => LineSpacing::Exact { size: Points(p.line) },
            LineRule::AtLeast => LineSpacing::AtLeast { size: Points(p.line) },
        })),
        space_before: fold(paras.iter().map(|(p, _)| if p.before_auto { AutoOr::Auto } else { AutoOr::Value(Points(p.space_before)) })),
        space_after: fold(paras.iter().map(|(p, _)| if p.after_auto { AutoOr::Auto } else { AutoOr::Value(Points(p.space_after)) })),
        indents: IndentState {
            left: pt(&|p| p.ind_left),
            right: pt(&|p| p.ind_right),
            first_line: pt(&|p| p.ind_first.max(0.0)),
            hanging: pt(&|p| (-p.ind_first).max(0.0)),
            mirror: tri(&|p| p.mirror_indents),
        },
        list: fold(paras.iter().map(|(_, l)| match l {
            Some(true) => ListState::Bullet,
            Some(false) => ListState::Numbered,
            None => ListState::None,
        })),
        list_level: fold(paras.iter().filter(|(_, l)| l.is_some()).filter_map(|(p, _)| p.num.map(|ListRef { list: ListId(_), level: l }| l as u32))),
        flow: FlowState {
            keep_with_next: tri(&|p| p.keep_next),
            keep_lines_together: tri(&|p| p.keep_lines),
            page_break_before: tri(&|p| p.page_break_before),
            widow_control: tri(&|p| p.widow_control),
            dont_hyphenate: tri(&|p| p.suppress_auto_hyphens),
            suppress_line_numbers: tri(&|p| p.suppress_line_numbers),
            textbox_tight_wrap: fold(paras.iter().map(|(p, _)| {
                use crate::api::command::TightWrap as T;
                [T::None, T::AllLines, T::FirstAndLastLine, T::FirstLineOnly, T::LastLineOnly]
                    .get(usize::from(p.tight_wrap))
                    .copied()
                    .unwrap_or_default()
            })),
        },
        borders: fold(paras.iter().map(|(p, _)| borders_of(p))),
        shading: fold(paras.iter().map(|(p, _)| p.shading.map_or(NoneOr::None, |c| NoneOr::Value(Color(c))))),
        contextual_spacing: tri(&|p| p.contextual),
        outline_level: fold(paras.iter().map(|(p, _)| match p.outline_lvl {
            Some(l @ 0..=8) => l as u8 + 1,
            _ => 0,
        })),
        collapsed: tri(&|p| p.collapsed),
        text_direction: fold(paras.iter().map(|(p, _)| {
            use crate::api::command::TextFlow as D;
            match p.text_flow {
                1 => D::TbRl,
                3 => D::BtLr,
                4 => D::LrTbV,
                5 => D::TbRlV,
                7 => D::TbLrV,
                _ => D::LrTb,
            }
        })),
        drop_cap: None,
        ..ParagraphFormatting::default()
    };
    super::readback::paragraph(&paras.iter().map(|(p, _)| *p).collect::<Vec<_>>(), crate::api::state::Tri::Unavailable).fill(&mut out);
    out
}

/// What the selection's formatting is read from.
pub(super) struct SelectionSources {
    /// The focus's paragraph.
    pub para: ParaId,
    /// The selected runs, at least one.
    pub runs: Vec<Run>,
    /// The selected paragraphs, at least one.
    pub paras: Vec<ParaId>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StateFlags {
    pub read_only: bool,
}

impl Editor {
    /// The selection's formatting sources: the focus's paragraph, the selected runs (none: the caret's run, else the
    /// paragraph mark's, alone) and the selected paragraphs.
    pub(super) fn selection_sources(&mut self) -> Option<SelectionSources> {
        let focus = self.selection.focus();
        let ranges: Vec<(ParaId, crate::layout::Unit, crate::layout::Unit)> =
            if self.selection.is_collapsed() && !self.is_discontiguous() { Vec::new() } else { self.selected_ranges() };
        let para_id = focus.and_then(|p| p.para_offset()).map(|(p, _)| p).or_else(|| match self.selection {
            Selection::Object(ObjectRef::Inline { para, .. }) => Some(para),
            Selection::Object(ObjectRef::Floating(f)) => self.index.float_anchor(f),
            Selection::Cells { .. } => ranges.first().map(|r| r.0),
            _ => None,
        })?;
        let mark = self.index.para(&self.doc, para_id)?.mark.clone();
        let mut runs: Vec<Run> = ranges.iter().filter(|r| r.1 < r.2).flat_map(|&r| self.runs_in(r)).collect();
        runs.dedup_by(|a, b| Rc::ptr_eq(a, b));
        if runs.is_empty() {
            runs.push(focus.and_then(|c| self.run_at(&c)).unwrap_or(mark));
        }
        let mut paras: Vec<ParaId> = ranges.iter().map(|r| r.0).collect();
        paras.dedup();
        if paras.is_empty() {
            paras.push(para_id);
        }
        Some(SelectionSources { para: para_id, runs, paras })
    }

    pub fn format_state(&mut self) -> Option<FormatState> {
        let sources = self.selection_sources()?;
        self.format_state_of(&sources)
    }

    pub(super) fn format_state_of(&mut self, sources: &SelectionSources) -> Option<FormatState> {
        let focus = self.selection.focus();
        let SelectionSources { para: para_id, runs, paras } = sources;
        let para_id = *para_id;
        let style = self.index.para(&self.doc, para_id)?.style.clone();
        let list = self.para_list(para_id);
        let run = runs[0].clone();
        // With list numbers selected, the number's formatting, as it draws in the clicked paragraph
        let label = self.list_label_selection().and_then(|l| self.index.para(&self.doc, l.para)).and_then(|p| p.list.as_ref()).map(|l| l.run.clone());
        let character = match label {
            Some(l) => self::character(&[&l]),
            None => self::character(&runs.iter().map(|r| &**r).collect::<Vec<_>>()),
        };
        let mut styles: Vec<(Rc<ParaStyle>, Option<bool>)> =
            paras.iter().filter_map(|&p| Some((self.index.para(&self.doc, p)?.style.clone(), self.para_list(p)))).collect();
        styles.dedup_by(|a, b| Rc::ptr_eq(&a.0, &b.0) && a.1 == b.1);
        let mut paragraph = self::paragraph(&styles.iter().map(|(s, l)| (&**s, *l)).collect::<Vec<_>>());
        // The tab stops: the first paragraph's, when every one has the same.
        paragraph.tabs = match styles.first() {
            Some((first, _)) if styles.iter().all(|(p, _)| p.tabs == first.tabs) => crate::api::state::Tri::Uniform(self.tab_stops(paras[0])),
            Some(_) => crate::api::state::Tri::Mixed,
            None => crate::api::state::Tri::Unavailable,
        };
        paragraph.drop_cap = self.drop_cap().filter(|d| d.position != crate::api::command::DropCapPosition::None);
        let level = focus.and_then(|p| self.index.chain_of(&p).last().copied());
        let table = match self.selection {
            Selection::Cells { table, .. } => Some(table),
            _ => level.map(|l| l.table),
        };
        let cell = match self.selection {
            Selection::Cells { table, anchor, .. } => {
                self.index.table(&self.doc, table).and_then(|t| super::table::Grid::of(t).cell_at(anchor.row, anchor.col)).map(|c| (anchor.row, c))
            }
            _ => level.filter(|l| Some(l.table) == table).map(|l| (l.row, l.cell)),
        };
        let object = self.selected_object();
        let story = focus.map_or(self.editing(), |p| p.story);
        let section = self.focus_section();
        Some(FormatState {
            run,
            character,
            paragraph,
            para: style,
            list,
            has_selection: !self.selection.is_collapsed(),
            can_undo: self.history.undo_depth() > 0,
            can_redo: self.history.redo_depth() > 0,
            table,
            object: object.as_ref().map(|o| o.kind),
            object_size: object.as_ref().map(|o| (o.width, o.height)),
            object_alt: object.map(|o| o.alt),
            cell,
            story,
            section,
        })
    }

    pub fn selected_object(&self) -> Option<SelectedObject> {
        match self.selection {
            Selection::Object(ObjectRef::Inline { para, offset }) => {
                match self.index.para(&self.doc, para).and_then(|p| super::stories::inline_at(p, offset)) {
                    Some(crate::docx::Inline::Picture(p, _)) => Some(SelectedObject {
                        kind: ObjectKind::Picture,
                        width: p.w,
                        height: p.h,
                        alt: p.alt.clone(),
                        decorative: p.shape.as_ref().is_some_and(|s| s.nv.decorative),
                        wrap: None,
                        locked: None,
                    }),
                    _ => None,
                }
            }
            Selection::Object(ObjectRef::Floating(f)) => {
                let p = self.index.float_anchor(f).and_then(|a| self.index.para(&self.doc, a))?;
                p.inlines.iter().find_map(|i| match i {
                    crate::docx::Inline::Float(fl, _) if crate::layout::FloatId(fl.id) == f => Some(SelectedObject {
                        kind: if fl.text_box.is_some() { ObjectKind::TextBox } else { ObjectKind::Float },
                        width: fl.w,
                        height: fl.h,
                        alt: fl.alt.clone(),
                        decorative: fl.shape.as_ref().is_some_and(|s| s.nv.decorative),
                        wrap: Some((fl.wrap, fl.behind)),
                        locked: Some(fl.locked),
                    }),
                    _ => None,
                })
            }
            _ => None,
        }
    }

    pub fn key_command(&mut self, code: &str, mods: Mods) -> Option<Command> {
        let cx = KeyContext {
            in_table: self.focus_chain_depth() > 0 || matches!(self.selection, Selection::Cells { .. }),
            list_start: self.tab_changes_level(),
            float: matches!(self.selection, Selection::Object(ObjectRef::Floating(_))),
            mac: self.mac,
            arrows: if code.starts_with("Arrow") { self.caret_arrows() } else { None },
        };
        // Tab and Shift+Tab move between legacy form fields where forms are filled in.
        if code == "Tab" && !mods.ctrl && !mods.alt && !mods.meta && self.tab_moves_between_form_fields() {
            let order = if mods.shift { crate::api::value::Order::Previous } else { crate::api::value::Order::Next };
            return Some(Command::SelectNextFormField(order));
        }
        match super::keymap::command(code, mods, cx)? {
            // Tab takes a list along from its first item as an AutoFormat option: off, it changes the level.
            Bound::Run(c @ (Command::IncreaseIndent | Command::DecreaseIndent)) if code == "Tab" && !self.indent_keys => {
                use crate::api::command::LevelChange;
                Some(Command::SetListLevel(if c == Command::IncreaseIndent { LevelChange::Increase } else { LevelChange::Decrease }))
            }
            Bound::Run(c) => Some(c),
            Bound::Toggle(t) => self.toggled(t),
        }
    }

    fn toggled(&mut self, t: Toggle) -> Option<Command> {
        if let Toggle::TrackChanges = t {
            return Some(Command::SetTrackChanges(!self.tracking()));
        }
        if let Toggle::FieldCodes { all } = t {
            return self.field_codes_toggle(all).map(Command::ShowFieldCodes);
        }
        if let Toggle::CaseCycle = t {
            return self.case_cycle().map(Command::ChangeCase);
        }
        if let Toggle::FormattingMarks = t {
            let marks = Some(!self.mark_options().all);
            return Some(Command::SetViewOptions(crate::api::command::ViewOptions { formatting_marks: marks, ..Default::default() }));
        }
        let f = self.format_state()?;
        let c = &f.character;
        let on = |v: &Tri<bool>| *v != Tri::Uniform(true);
        Some(match t {
            Toggle::Bold => Command::SetBold(on(&c.bold)),
            Toggle::Italic => Command::SetItalic(on(&c.italic)),
            Toggle::Underline => {
                Command::SetUnderline(if c.underline == Tri::Uniform(UnderlineStyle::Single) { UnderlineStyle::None } else { UnderlineStyle::Single })
            }
            Toggle::DoubleUnderline | Toggle::WordUnderline => {
                let u = if t == Toggle::DoubleUnderline { UnderlineStyle::Double } else { UnderlineStyle::Words };
                Command::SetUnderline(if c.underline == Tri::Uniform(u) { UnderlineStyle::None } else { u })
            }
            Toggle::Subscript => Command::SetSubscript(on(&c.subscript)),
            Toggle::Superscript => Command::SetSuperscript(on(&c.superscript)),
            Toggle::SmallCaps => Command::SetSmallCaps(on(&c.small_caps)),
            Toggle::AllCaps => Command::SetAllCaps(on(&c.all_caps)),
            Toggle::Hidden => Command::SetHidden(on(&c.hidden)),
            Toggle::Align(a) => Command::SetAlignment(if f.paragraph.alignment == Tri::Uniform(a) { Alignment::Left } else { a }),
            Toggle::SpaceBefore => {
                let some = f.para.space_before > 0.0;
                Command::SetParagraphSpacing(ParagraphSpacing {
                    before: Some(AutoOr::Value(Points(if some { 0.0 } else { 12.0 }))),
                    after: None,
                    contextual: None,
                })
            }
            Toggle::TrackChanges | Toggle::FieldCodes { .. } | Toggle::CaseCycle | Toggle::FormattingMarks => return None,
        })
    }

    pub fn command_states(&mut self, f: Option<&FormatState>, flags: StateFlags) -> BTreeMap<CommandId, CommandState> {
        use CommandId as C;
        let editable = !flags.read_only;
        let formatted = editable && f.is_some();
        // Character formatting, which a floating object doesn't take
        let chars = formatted && !matches!(self.selection, Selection::Object(ObjectRef::Floating(_)));
        let selected = !self.selection.is_collapsed();
        let in_table = f.is_some_and(|f| f.table.is_some());
        let story = f.map_or(StoryId::Body, |f| f.story);
        let body_text = editable && story == StoryId::Body && !in_table;
        let object = f.and_then(|f| f.object);
        let floating = matches!(object, Some(ObjectKind::Float | ObjectKind::TextBox));
        let shape = self.shape_info();
        let in_list = f.is_some_and(|f| f.list.is_some());
        let styled = self.style_target.is_some();
        let paras = if styled { Vec::new() } else { self.selected_paras() };
        let listed = styled
            || paras
                .iter()
                .any(|&p| self.index.para(&self.doc, p).is_some_and(|p| p.style.num.is_some_and(|ListRef { list: ListId(id), level: _ }| id != 0)));
        let framed = styled || self.frame_here().is_some();
        let tab_stops = self.tab_stops_in(&paras);
        let notes = self.takes_notes();
        let commentable = self.takes_comments_here();
        let section = self.focus_section();
        let header_footer = matches!(story, StoryId::HeaderFooter(_));
        let can_merge = matches!(self.selection, Selection::Cells { anchor, focus, .. } if anchor != focus);
        let link = self.link_here().is_some();
        let comments = !self.doc.comments.by_id.is_empty();
        let frame_object = matches!(self.selection, Selection::Object(ObjectRef::Floating(f)) if self.float_of(f).is_some_and(|fl| fl.is_frame()));
        let back = !self.edit_marks.is_empty();
        let repeat =
            f.is_some_and(|f| f.can_redo) || self.repeat.as_ref().is_some_and(|c| !(self.is_discontiguous() && super::multi::single_only(c.id())));
        let tracking = self.tracking();
        let locked = self.doc.settings.protection.as_ref().is_some_and(|p| p.forces_tracking());
        let (undo_label, redo_label) = (self.history.next_undo().map(str::to_string), self.history.next_redo().map(str::to_string));
        let toggle = |v: Tri<bool>| (Some(v == Tri::Uniform(true)), Some(CommandValue::Toggle(v)));
        let mut out = BTreeMap::new();
        for info in catalog() {
            let id = info.id;
            let (enabled, (checked, value)) = match (id, f) {
                _ if super::commands::UNSUPPORTED.contains(&id) => (false, (None, None)),
                (C::InsertPageBreak | C::InsertColumnBreak | C::InsertSectionBreak, _) => (body_text, (None, None)),
                (C::Cut, _) => (editable && selected, (None, None)),
                (C::Copy, _) => (selected, (None, None)),
                (C::Paste, _) => (editable, (None, None)),
                (C::Repaste, _) => (editable && self.last_paste().is_some(), (None, None)),
                (C::DismissPasteOptions, _) => (self.last_paste().is_some(), (None, None)),
                (C::SetPasteOptions | C::SetAutoCaption | C::GoTo, _) => (true, (None, None)),
                (C::CopyFormatting, _) => (f.is_some(), (None, None)),
                (C::PasteFormatting, _) => (formatted && self.painter.is_some(), (None, None)),
                (C::Undo, _) => {
                    (editable && undo_label.is_some(), (None, Some(CommandValue::Text(undo_label.clone().map_or(Tri::Unavailable, Tri::Uniform)))))
                }
                (C::Redo, _) => {
                    (editable && redo_label.is_some(), (None, Some(CommandValue::Text(redo_label.clone().map_or(Tri::Unavailable, Tri::Uniform)))))
                }
                (C::Repeat, _) => (editable && repeat, (None, None)),
                (C::MoveToCell, _) => (in_table, (None, None)),
                (C::GoBack, _) => (back, (None, None)),
                (C::SetSelectionMode, _) => {
                    let mode = match (self.modes.extend > 0, self.modes.block) {
                        (_, true) => crate::api::command::SelectionMode::Block,
                        (true, _) => crate::api::command::SelectionMode::Extend,
                        _ => crate::api::command::SelectionMode::Normal,
                    };
                    (true, (None, Some(CommandValue::SelectionMode(mode))))
                }
                (C::SetBold, Some(f)) => (chars, toggle(f.character.bold)),
                (C::SetItalic, Some(f)) => (chars, toggle(f.character.italic)),
                (C::SetStrikethrough, Some(f)) => (chars, toggle(f.character.strikethrough)),
                (C::SetDoubleStrikethrough, Some(f)) => (chars, toggle(f.character.double_strikethrough)),
                (C::SetSubscript, Some(f)) => (chars, toggle(f.character.subscript)),
                (C::SetSuperscript, Some(f)) => (chars, toggle(f.character.superscript)),
                (C::SetSmallCaps, Some(f)) => (chars, toggle(f.character.small_caps)),
                (C::SetAllCaps, Some(f)) => (chars, toggle(f.character.all_caps)),
                (C::SetHidden, Some(f)) => (chars, toggle(f.character.hidden)),
                (C::SetUnderline, Some(f)) => (chars, (None, Some(CommandValue::Underline(f.character.underline)))),
                (C::SetFont, Some(f)) => (chars, (None, Some(CommandValue::Text(f.character.font.clone())))),
                (C::SetFontSize, Some(f)) => (chars, (None, Some(CommandValue::Length(f.character.size)))),
                (C::SetFontColor, Some(f)) => (chars, (None, Some(CommandValue::Color(f.character.color)))),
                (C::SetHighlight, Some(f)) => (chars, (None, Some(CommandValue::Highlight(f.character.highlight)))),
                (C::SetCharacterSpacing, Some(f)) => (chars, (None, Some(CommandValue::Length(f.character.character_spacing)))),
                (C::SetCharacterPosition, Some(f)) => (chars, (None, Some(CommandValue::Length(f.character.position)))),
                (
                    C::ApplyCharacterStyle
                    | C::SetTypography
                    | C::SetLanguage
                    | C::SetCharacterEffects
                    | C::SetTextEffects
                    | C::SetUnderlineColor
                    | C::SetOutline,
                    Some(_),
                ) => (chars, (None, None)),
                (C::SetParagraphTextDirection, Some(_)) => (formatted, (None, None)),
                (C::ChangeCase, _) => (editable && selected, (None, None)),
                (C::SetAlignment, Some(f)) => (formatted, (None, Some(CommandValue::Alignment(f.paragraph.alignment)))),
                (C::SetLineSpacing, Some(f)) => (formatted, (None, Some(CommandValue::LineSpacing(f.paragraph.line_spacing)))),
                (C::SetParagraphStyle, Some(f)) => {
                    let style = match &f.paragraph.style {
                        Tri::Uniform(s) => Tri::Uniform(s.0.clone()),
                        Tri::Mixed => Tri::Mixed,
                        Tri::Unavailable => Tri::Unavailable,
                    };
                    (formatted, (None, Some(CommandValue::Text(style))))
                }
                (C::SetList, Some(f)) => (formatted, (None, Some(CommandValue::List(f.paragraph.list)))),
                (C::SetListLevel, Some(f)) => {
                    let level = match f.paragraph.list_level {
                        Tri::Uniform(l) => Tri::Uniform(f64::from(l)),
                        Tri::Mixed => Tri::Mixed,
                        Tri::Unavailable => Tri::Unavailable,
                    };
                    (formatted && in_list, (None, Some(CommandValue::Number(level))))
                }
                (C::EditHeader | C::EditFooter, _) => (editable && story == StoryId::Body, (None, None)),
                (
                    C::SetMargins
                    | C::SetOrientation
                    | C::SetPaperSize
                    | C::SetColumns
                    | C::MoveColumnBoundary
                    | C::SetHeaderFooterDistance
                    | C::SetDifferentFirstPage
                    | C::SetPageNumbering
                    | C::SetPageVerticalAlignment
                    | C::SetLineNumbering
                    | C::SetSectionBreakType
                    | C::SetSectionTextDirection
                    | C::SetPageBorders
                    | C::SetHeaderFooterLinked
                    | C::RemoveHeaderFooter
                    | C::InsertPageNumber,
                    _,
                ) if self.editing_sub_template().is_some() => (false, (None, None)),
                (C::CloseSubTemplate, _) => (self.editing_sub_template().is_some(), (None, None)),
                (C::EditSubTemplate, _) => (self.has_sub_templates(), (None, None)),
                (C::CloseHeaderFooter, _) => (header_footer, (None, None)),
                (C::EditHyperlink | C::RemoveHyperlink, _) => (editable && link, (None, None)),
                (C::MergeCells, _) => (editable && can_merge, (None, None)),
                (
                    C::InsertRows
                    | C::InsertColumns
                    | C::DeleteRows
                    | C::DeleteColumns
                    | C::DeleteTable
                    | C::DeleteCells
                    | C::SplitCells
                    | C::SplitTable
                    | C::ConvertTableToText
                    | C::DistributeRows
                    | C::DistributeColumns
                    | C::MoveRows
                    | C::AutoFitColumns
                    | C::SetCellAlignment
                    | C::SetCellShading
                    | C::SetCellBorders
                    | C::SetCellMargins
                    | C::SetCellProperties
                    | C::SetRowProperties
                    | C::SetTableCellSpacing
                    | C::SetTableAltText
                    | C::SetHeaderRow
                    | C::SetRowHeight
                    | C::SetRowCanBreak
                    | C::SetColumnWidth
                    | C::SetTableWidth
                    | C::SetTableAlignment
                    | C::SetTableIndent
                    | C::SetTableStyle
                    | C::SetTableStyleOptions
                    | C::SetTableBandSizes
                    | C::SetTableCellMargins
                    | C::InsertCells,
                    _,
                ) => (editable && in_table, (None, None)),
                (C::DrawTableStroke | C::EraseTableBorder, _) => (editable, (None, None)),
                (C::SetTableTool | C::SetTablePen, _) => (true, (None, None)),
                (C::SetObjectName | C::SetObjectTitle | C::SetLockAspectRatio, _) => (editable && object.is_some() && !frame_object, (None, None)),
                (C::SetObjectHidden | C::RestackObject, _) => (editable, (None, None)),
                (C::SelectObject, _) => (true, (None, None)),
                (C::SetPictureRecolor | C::ResetPicture, _) => (editable && shape.as_ref().is_some_and(|s| s.pic), (None, None)),
                (C::SetWrapPoints | C::ResetWrapPoints, _) => (editable && self.wrap_points_target().is_ok(), (None, None)),
                (C::LinkTextBox | C::BeginTextBoxLink, _) => {
                    let last = self.text_box_link().is_none_or(|l| l.next.is_none());
                    (editable && object == Some(ObjectKind::TextBox) && last, ((id == C::BeginTextBoxLink).then_some(self.linking()), None))
                }
                (C::UnlinkTextBox, _) => (editable && self.text_box_link().is_some_and(|l| l.next.is_some()), (None, None)),
                (C::SetSelectObjects, _) => (true, (Some(self.selecting_objects()), None)),
                (C::SetTablePosition, _) => (editable && in_table, (None, self.table_position().map(CommandValue::TablePosition))),
                (C::SetObjectSize | C::SetObjectWrap | C::SetAltText | C::DeleteObject, _) => (editable && object.is_some(), (None, None)),
                (C::AlignObjects, _) => (editable && floating, (None, None)),
                (C::SetFormFieldShading, _) => (editable, (Some(!self.doc.settings.no_form_shading), None)),
                (C::SetPictureCrop, _) => (editable && shape.as_ref().is_some_and(|s| s.pic && s.child_path.is_empty()), (None, None)),
                (C::ChangePicture, _) => (editable && matches!(object, Some(ObjectKind::Picture | ObjectKind::Float)), (None, None)),
                (C::ResetObjectSize, _) => (editable && matches!(object, Some(ObjectKind::Picture | ObjectKind::Float)), (None, None)),
                (C::SetAnchorLocked | C::SetObjectPosition | C::SetObjectWrapOptions, _) => (editable && floating, (None, None)),
                (C::SetShapePreset | C::SetShapeText, _) => (editable && shape.as_ref().is_some_and(|s| s.sp), (None, None)),
                // A picture's geometry has adjustments too (a rounded rectangle's corners)
                (C::SetShapeAdjust, _) => (
                    editable && shape.as_ref().is_some_and(|s| s.sp || (s.pic && s.state.as_ref().is_some_and(|st| !st.adjust.is_empty()))),
                    (None, None),
                ),
                (C::SetShapeFill | C::SetShapeEffects, _) => (editable && shape.is_some(), (None, None)),
                (C::SetShapeLine, _) => (editable && shape.as_ref().is_some_and(|s| s.sp || s.pic), (None, None)),
                (C::SetPictureAdjust, _) => (editable && shape.as_ref().is_some_and(|s| s.pic), (None, None)),
                (C::SetObjectRotation | C::FlipObject, _) => (editable && shape.is_some(), (None, None)),
                (C::ArrangeObject, _) => (editable && floating, (None, None)),
                (C::GroupObjects, _) => (editable && self.selected_floats().len() >= 2, (None, None)),
                (C::UngroupObjects, _) => (editable && shape.as_ref().is_some_and(|s| s.group && s.child_path.is_empty()), (None, None)),
                (C::SelectGroupChild, _) => {
                    let in_group = shape.as_ref().is_some_and(|s| s.group || !s.child_path.is_empty());
                    (in_group, (None, None))
                }
                (C::SetTrackChanges, _) => (editable && !(locked && tracking), (Some(tracking), None)),
                (C::SetMarkupView, _) => (true, (None, Some(CommandValue::MarkupView(self.review.view)))),
                (C::SetTrackOptions | C::AcceptAllAndStopTracking, _) => (editable && !locked, (None, None)),
                (C::DeleteAllComments, _) => (editable && comments, (None, None)),
                (C::PromoteOutline | C::DemoteOutline | C::PromoteToHeading1 | C::DemoteToBodyText, _) => {
                    (formatted && !matches!(self.selection, Selection::Object(_)) && self.style_target.is_none(), (None, None))
                }
                (
                    C::PromoteHeading
                    | C::DemoteHeading
                    | C::MoveHeading
                    | C::DeleteHeading
                    | C::InsertHeadingBefore
                    | C::InsertHeadingAfter
                    | C::InsertSubheading,
                    _,
                ) => (editable && self.style_target.is_none(), (None, None)),
                (C::SetActiveComment | C::SelectComment, _) => (comments, (None, None)),
                (C::ReplyToComment | C::EditComment | C::DeleteComment | C::ResolveComment, _) => (editable && comments, (None, None)),
                (C::SetAutoHyphenation, _) => (editable, (Some(self.doc.settings.auto_hyphenation), None)),
                (C::SetHyphenateCaps, _) => (editable, (Some(!self.doc.settings.do_not_hyphenate_caps), None)),
                (C::SetHyphenationZone, _) => {
                    (editable, (None, Some(CommandValue::Length(Tri::Uniform(Points(self.doc.settings.hyphenation_zone))))))
                }
                (C::SetConsecutiveHyphenLimit, _) => {
                    (editable, (None, Some(CommandValue::Number(Tri::Uniform(self.doc.settings.consecutive_hyphen_limit as f64)))))
                }
                (C::SetDifferentFirstPage, Some(f)) => {
                    let on = self.doc.sections.get(f.section).is_some_and(|s| s.title_pg);
                    (editable, (Some(on), None))
                }
                (C::RestartNumbering | C::ContinueNumbering | C::SetNumberingValue | C::AdjustListIndents, _) => (editable && listed, (None, None)),
                (C::SetFrame | C::RemoveFrame, _) => (editable && framed, (None, None)),
                (C::ClearTabStops, _) => (editable && tab_stops, (None, None)),
                (C::InsertFootnote | C::InsertEndnote | C::InsertNote, _) => (editable && notes, (None, None)),
                (C::AddComment, _) => (editable && commentable, (None, None)),
                (C::MoveText | C::CopyText, _) => (editable && selected && matches!(self.selection, Selection::Text { .. }), (None, None)),
                (C::SelectField, _) => (true, (None, None)),
                (C::ShrinkSelection, _) => (self.modes.extend >= 2, (None, None)),
                (C::SelectSimilarFormatting, _) => (self.selection.focus().is_some(), (None, None)),
                (C::SelectStyleInstances, _) => (self.doc.definitions.is_some(), (None, None)),
                (C::ToggleCharacterCode | C::GrowFontOnePoint | C::ShrinkFontOnePoint | C::ClearAllFormatting | C::SetFitText, _) => {
                    (chars, (None, None))
                }
                (C::InsertStyleSeparator | C::IncreaseHangingIndent | C::DecreaseHangingIndent | C::ClearParagraphFormatting, _) => {
                    (formatted, (None, None))
                }
                (C::MoveParagraphs, _) => (formatted && !matches!(self.selection, Selection::Object(_) | Selection::Cells { .. }), (None, None)),
                (C::RegroupObjects, _) => (editable && self.regroupable().is_some(), (None, None)),
                (C::EditWrapPoints, _) => (editable && self.wrap_points_target().is_ok(), (Some(self.editing_wrap_points()), None)),
                (C::SetHeaderFooterLinked, _) => (editable && section > 0, (None, None)),
                (_, None) if info.mutates => (false, (None, None)),
                _ => (editable || !info.mutates, (None, None)),
            };
            out.insert(id, CommandState { enabled, checked, value });
        }
        // A discontiguous selection
        if self.is_discontiguous() {
            let cells = editable && self.any_piece_in_table();
            for (id, s) in out.iter_mut() {
                if super::multi::single_only(*id) {
                    s.enabled = false;
                } else if super::multi::formats_cells(*id) {
                    s.enabled = cells;
                }
            }
        }
        // What document protection refuses here.
        if let Some(refuses) = self.protection_gate() {
            out.iter_mut().filter(|(id, s)| s.enabled && refuses(**id)).for_each(|(_, s)| s.enabled = false);
        }
        out
    }

    pub fn table_selection(&self) -> Option<TableSelection> {
        let tg = self.table_target()?;
        let t = self.index.table(&self.doc, tg.table)?;
        let g = super::table::Grid::of(t);
        let (r0, r1) = (tg.rows.0, tg.rows.1.min(t.rows.len().saturating_sub(1)));
        let rows = &t.rows[r0..=r1];
        let cells: Vec<&crate::docx::Cell> = (r0..=r1)
            .flat_map(|r| {
                let g = &g;
                t.rows[r].cells.iter().enumerate().filter(move |(k, _)| {
                    g.rows.get(r).and_then(|row| row.get(*k)).is_some_and(|&(c, s, _)| c + s.max(1) > tg.cols.0 && c <= tg.cols.1)
                })
            })
            .map(|(_, c)| c)
            .collect();
        let focus = self.selection.focus().and_then(|p| self.index.chain_of(&p).last().copied()).filter(|l| l.table == tg.table);
        let (row, column) = focus.map_or((r0, tg.cols.0), |l| (l.row, g.col_of(l.row, l.cell).unwrap_or(0)));
        let look = t.look.unwrap_or(crate::docx::props::Look {
            first_row: true,
            last_row: false,
            first_col: true,
            last_col: false,
            no_h_band: false,
            no_v_band: true,
        });
        Some(TableSelection {
            rows: t.rows.len() as u32,
            columns: g.cols as u32,
            row: row as u32,
            column: column as u32,
            can_merge: matches!(self.selection, Selection::Cells { anchor, focus, .. } if anchor != focus),
            header_row: fold(rows.iter().map(|r| r.header)),
            row_can_break: fold(rows.iter().map(|r| !r.cant_split)),
            cell_alignment: fold(cells.iter().map(|c| super::table_format::api_cell_alignment(Some(c.v_align)))),
            cell_shading: fold(cells.iter().map(|c| c.shading.map_or(NoneOr::None, |s| NoneOr::Value(Color(s))))),
            row_height: fold(rows.iter().map(|r| match &r.height {
                None => AutoOr::Auto,
                Some((h, rule)) => AutoOr::Value(RowHeight {
                    height: Points(*h),
                    rule: if *rule == crate::docx::HeightRule::Exact { HeightRule::Exact } else { HeightRule::AtLeast },
                }),
            })),
            column_width: fold((tg.cols.0..=tg.cols.1).filter_map(|c| t.grid.get(c)).map(|w| Points(*w))),
            style: t.style_id.clone().map(StyleId),
            style_options: TableStyleOptions {
                header_row: look.first_row,
                total_row: look.last_row,
                first_column: look.first_col,
                last_column: look.last_col,
                banded_rows: !look.no_h_band,
                banded_columns: !look.no_v_band,
            },
            width: match t.width {
                crate::docx::props::Width::Dxa(v) => TableWidth::Points { value: Points(v) },
                crate::docx::props::Width::Pct(f) => TableWidth::Percent { value: f * 100.0 },
                _ => TableWidth::Auto,
            },
            alignment: match t.jc {
                Jc::Center => HorizontalAlignment::Center,
                Jc::Right => HorizontalAlignment::Right,
                _ => HorizontalAlignment::Left,
            },
            indent: Points(t.ind),
            cell_width: fold(cells.iter().map(|c| match c.width {
                Some(crate::docx::props::Width::Dxa(v)) => TableWidth::Points { value: Points(v) },
                Some(crate::docx::props::Width::Pct(f)) => TableWidth::Percent { value: f * 100.0 },
                _ => TableWidth::Auto,
            })),
            text_direction: fold(
                cells.iter().map(|c| serde_json::from_value(serde_json::Value::String(c.text_direction.clone())).unwrap_or_default()),
            ),
            wrap_text: fold(cells.iter().map(|c| !c.no_wrap)),
            fit_text: fold(cells.iter().map(|c| c.fit_text)),
            row_hidden: fold(rows.iter().map(|r| r.hidden)),
            cell_spacing: (t.cell_spacing > 0.0).then_some(Points(t.cell_spacing)),
            title: t.alt.as_ref().map(|a| a.title.clone()).unwrap_or_default(),
            description: t.alt.as_ref().map(|a| a.description.clone()).unwrap_or_default(),
            bands: self.table_band_sizes().unwrap_or_default(),
            ..self.table_readback().map(super::table_readback::TableReadback::into_state).unwrap_or_default()
        })
    }

    pub fn object_selection(&self) -> Option<ObjectSelection> {
        let o = self.selected_object()?;
        let wrap = match o.wrap {
            None => ObjectWrap::Inline,
            Some((FloatWrap::Square, _)) => ObjectWrap::Square,
            Some((FloatWrap::Tight, _)) => ObjectWrap::Tight,
            Some((FloatWrap::Through, _)) => ObjectWrap::Through,
            Some((FloatWrap::TopAndBottom, _)) => ObjectWrap::TopAndBottom,
            Some((FloatWrap::None, true)) => ObjectWrap::BehindText,
            Some((FloatWrap::None, false)) => ObjectWrap::InFrontOfText,
        };
        let shape = self.shape_info().unwrap_or_default();
        Some(ObjectSelection {
            kind: shape.kind.unwrap_or(if o.kind == ObjectKind::TextBox {
                crate::api::state::ObjectKind::TextBox
            } else {
                crate::api::state::ObjectKind::Picture
            }),
            wrap,
            width: Points(o.width),
            height: Points(o.height),
            alt_text: o.alt.to_string(),
            decorative: o.decorative,
            anchor_locked: o.locked,
            placement: self.object_placement(),
            shape: shape.state,
            rotation: shape.rotation,
            flip_h: shape.flip_h,
            flip_v: shape.flip_v,
            child_path: shape.child_path,
            count: shape.count.max(1),
            crop: self.selected_picture_crop(),
            transparent_color: self.selected_transparent_color(),
            relative_width: self.selected_relative_size(0),
            relative_height: self.selected_relative_size(1),
            ..ObjectSelection::default()
        })
    }

    pub fn element_context(&self) -> Vec<ElementContext> {
        super::elements::context(&self.doc, &self.index, &self.selection).into_iter().map(|(e, exact)| self.element_context_of(e, exact)).collect()
    }

    /// An element's context: what it is, its field code or control tag and title.
    pub fn element_context_of(&self, e: crate::docx::ElementId, exact: bool) -> ElementContext {
        let para = match self.locate(e) {
            Some(super::merge::Where::Inline { para, .. } | super::merge::Where::Sym { para, .. }) => self.index.para(&self.doc, para),
            _ => None,
        };
        let id = super::merge::api_id(e);
        match e {
            crate::docx::ElementId::Field(f) => {
                let code = para.and_then(|p| {
                    let k = p.inlines.iter().position(|i| matches!(i, crate::docx::Inline::FieldBegin { id, .. } if *id == f))?;
                    let s = crate::docx::fields::span_at(&p.inlines, k)?;
                    Some(crate::docx::fields::code_text(&p.inlines, s).trim().to_string())
                });
                let begin = para.and_then(|p| p.inlines.iter().find(|i| matches!(i, crate::docx::Inline::FieldBegin { id, .. } if *id == f)));
                let (mut locked, mut showing_code, mut dirty, mut form) = (false, false, false, None);
                if let Some(crate::docx::Inline::FieldBegin { lock, dirty: d, view, form: ff, .. }) = begin {
                    (locked, showing_code, dirty) = (*lock, *view == crate::docx::FieldView::Code, *d);
                    form = ff.as_ref().map(|ff| Box::new(super::forms::info(&ff.data)));
                }
                ElementContext {
                    id,
                    kind: ElementKind::Field,
                    exact,
                    locked,
                    showing_code,
                    dirty,
                    code,
                    tag: None,
                    title: None,
                    size: None,
                    form,
                    control: None,
                }
            }
            crate::docx::ElementId::Control(c) => {
                let props = self.doc.controls.get(&c).map(|k| k.props.clone()).unwrap_or_default();
                let kind = control_kind(&props);
                let mut info = control_info(&props);
                if let Some(crate::docx::KeptKind::Gallery { gallery, category }) = props.kept.clone() {
                    info.building_blocks = self.gallery_entries(gallery.as_deref(), category.as_deref()).map(|b| b.name.clone()).collect();
                }
                // A date control's date, from the text it shows
                if matches!(props.kind, crate::docx::ControlKind::Date { .. }) && !props.showing_placeholder {
                    let text = para.and_then(|p| {
                        let k = p.inlines.iter().position(|i| *i == crate::docx::Inline::ControlStart(c))?;
                        let end = p.inlines[k..].iter().position(|i| *i == crate::docx::Inline::ControlEnd(c)).map_or(p.inlines.len(), |e| k + e);
                        Some(crate::docx::fields::visible(&p.inlines[k + 1..end]))
                    });
                    info.date = text.and_then(|t| crate::docx::binding::full_date(&props, &t));
                }
                let control = Some(Box::new(info));
                ElementContext {
                    id,
                    kind,
                    exact,
                    locked: !props.lock.editable(),
                    showing_code: false,
                    dirty: false,
                    code: None,
                    tag: props.tag,
                    title: props.alias,
                    size: self.picture_control_size(c),
                    form: None,
                    control,
                }
            }
        }
    }

    pub fn link_info(&self) -> Option<LinkInfo> {
        self.link_here().map(|l| LinkInfo {
            address: l.address,
            text: l.text,
            target: l.target,
            bookmark: l.bookmark,
            screen_tip: l.tip,
            frame: l.frame,
        })
    }

    pub fn header_footer_info(&mut self, page: u32) -> Option<HeaderFooterInfo> {
        let StoryId::HeaderFooter(k) = self.selection.focus()?.story else { return None };
        let part = self.index.part(k)?;
        let (section, kind, variant) = self.doc.sections.iter().enumerate().find_map(|(i, s)| {
            let at = |refs: &crate::docx::HfRefs| refs.iter().position(|r| *r == Some(part));
            at(&s.headers).map(|v| (i, HeaderFooterKind::Header, v)).or_else(|| at(&s.footers).map(|v| (i, HeaderFooterKind::Footer, v)))
        })?;
        let pages = [HeaderFooterType::Default, HeaderFooterType::First, HeaderFooterType::Even][variant.min(2)];
        let what = if kind == HeaderFooterKind::Header { "Header" } else { "Footer" };
        let prefix = match pages {
            HeaderFooterType::First => "First Page ",
            HeaderFooterType::Even => "Even Page ",
            HeaderFooterType::Default if self.doc.settings.even_and_odd => "Odd Page ",
            HeaderFooterType::Default => "",
        };
        let label = format!("{prefix}{what} -Section {}-", section + 1);
        Some(HeaderFooterInfo { kind, pages, section: section as u32 + 1, page, label })
    }

    pub fn page_formatting(&self, section: usize) -> PageFormatting {
        let Some(s) = self.doc.sections.get(section) else { return PageFormatting::default() };
        let landscape = s.page_w > s.page_h;
        PageFormatting {
            size: PageSize { width: Points(s.page_w), height: Points(s.page_h) },
            orientation: if landscape { Orientation::Landscape } else { Orientation::Portrait },
            margins: PageMargins {
                top: Points(s.top),
                bottom: Points(s.bottom),
                left: Points(s.left),
                right: Points(s.right),
                gutter: Points(s.gutter),
            },
            columns: ColumnLayout {
                count: s.columns.num.max(1) as u32,
                spacing: Points(s.columns.space),
                separator: s.columns.separator,
                widths: if s.columns.equal {
                    Vec::new()
                } else {
                    s.columns.cols.iter().map(|&(w, sp)| crate::api::command::ColumnWidth { width: Points(w), spacing: Points(sp) }).collect()
                },
                equal: s.columns.equal,
            },
            header_distance: Points(s.header),
            footer_distance: Points(s.footer),
            different_first_page: s.title_pg,
            different_odd_even: self.doc.settings.even_and_odd,
            page_number_format: s.pg_fmt.as_deref().map_or(NumberFormat::Decimal, NumberFormat::of_val),
            page_number_start: s.pg_start.and_then(|n| u32::try_from(n).ok()),
            chapter_numbers: s.chapter.as_ref().and_then(|(level, sep)| {
                use crate::api::command::ChapterSeparator as S;
                let separator = match sep.as_str() {
                    "." => S::Period,
                    ":" => S::Colon,
                    "\u{2014}" => S::EmDash,
                    "\u{2013}" => S::EnDash,
                    _ => S::Hyphen,
                };
                Some(crate::api::command::ChapterNumbers { level: u8::try_from(*level).ok()?, separator })
            }),
            vertical_alignment: match s.v_align {
                crate::docx::VerticalAlignment::Center => PageVerticalAlignment::Center,
                crate::docx::VerticalAlignment::Both => PageVerticalAlignment::Justified,
                crate::docx::VerticalAlignment::Bottom => PageVerticalAlignment::Bottom,
                crate::docx::VerticalAlignment::Top => PageVerticalAlignment::Top,
            },
            line_numbers: s.line_numbers.as_ref().map(|l| crate::api::command::LineNumbers {
                start: u32::try_from(l.start + 1).ok(),
                count_by: u32::try_from(l.count_by).ok(),
                distance: (l.distance > 0.0).then_some(Points(l.distance)),
                restart: Some(match l.restart {
                    crate::docx::LineNumberingRestart::NewSection => crate::api::command::LineNumberRestart::EachSection,
                    crate::docx::LineNumberingRestart::Continuous => crate::api::command::LineNumberRestart::Continuous,
                    crate::docx::LineNumberingRestart::NewPage | crate::docx::LineNumberingRestart::Unrecognised => {
                        crate::api::command::LineNumberRestart::EachPage
                    }
                }),
            }),
            break_type: match s.break_kind {
                crate::docx::SectionBreak::NextPage => SectionBreakKind::NextPage,
                crate::docx::SectionBreak::Continuous => SectionBreakKind::Continuous,
                crate::docx::SectionBreak::EvenPage => SectionBreakKind::EvenPage,
                crate::docx::SectionBreak::OddPage => SectionBreakKind::OddPage,
                crate::docx::SectionBreak::NextColumn => SectionBreakKind::NextColumn,
            },
            text_direction: match s.text_flow {
                1 => TextFlow::TbRl,
                3 => TextFlow::BtLr,
                4 => TextFlow::LrTbV,
                5 => TextFlow::TbRlV,
                7 => TextFlow::TbLrV,
                _ => TextFlow::LrTb,
            },
            multiple_pages: match (self.doc.settings.mirror_margins, self.doc.settings.book_fold) {
                (_, Some(true)) => MultiplePagesMode::ReverseBookFold,
                (_, Some(false)) => MultiplePagesMode::BookFold,
                (true, None) => MultiplePagesMode::MirrorMargins,
                (false, None) => MultiplePagesMode::Normal,
            },
            gutter_at_top: self.doc.settings.gutter_at_top,
            // Linked: none of its own, and a section before to take it from.
            header_links: links(section > 0, &s.headers),
            footer_links: links(section > 0, &s.footers),
            footnotes: note_settings(
                &s.notes[0],
                if s.notes[0].beneath { NotePosition::BeneathText } else { NotePosition::PageBottom },
                NumberFormat::Decimal,
            ),
            endnotes: note_settings(
                &s.notes[1],
                if self.doc.settings.endnotes_sect_end { NotePosition::SectionEnd } else { NotePosition::DocumentEnd },
                NumberFormat::LowerRoman,
            ),
            footnote_columns: s.footnote_columns,
            background: self.background_state(),
            watermark: self.watermark_state(section),
            page_borders: self.page_borders_of(section).map(Into::into).unwrap_or_default(),
        }
    }
}

/// Which of a section's header or footer slots take the section before's.
fn links(has_before: bool, refs: &crate::docx::HfRefs) -> crate::api::state::HeaderFooterLinks {
    let l = |k: usize| has_before && refs[k].is_none();
    crate::api::state::HeaderFooterLinks { default: l(0), first: l(1), even: l(2) }
}

/// A section's note settings as state reports them; `default` is the kind's format when none is set.
fn note_settings(n: &crate::docx::NoteProps, position: NotePosition, default: NumberFormat) -> crate::api::state::NoteSettings {
    crate::api::state::NoteSettings {
        position,
        format: n.fmt.as_deref().map_or(default, NumberFormat::of_val),
        start: u32::try_from(n.start).unwrap_or(1),
        restart: match n.restart {
            crate::docx::NoteRestart::Continuous => crate::api::command::NoteRestart::Continuous,
            crate::docx::NoteRestart::EachSect => crate::api::command::NoteRestart::EachSection,
            crate::docx::NoteRestart::EachPage => crate::api::command::NoteRestart::EachPage,
        },
    }
}

/// A content control's public kind.
pub fn control_kind(p: &crate::docx::ControlProps) -> ElementKind {
    use crate::docx::{ControlKind as K, KeptKind};
    match p.kind {
        K::PlainText { .. } => ElementKind::PlainText,
        K::Picture => ElementKind::Picture,
        K::Checkbox { .. } => ElementKind::Checkbox,
        K::Date { .. } => ElementKind::Date,
        K::DropDown if p.combo => ElementKind::ComboBox,
        K::DropDown => ElementKind::DropDown,
        K::Group => ElementKind::Group,
        K::RepeatingSection { .. } => ElementKind::RepeatingSection,
        K::RepeatingItem => ElementKind::RepeatingItem,
        K::RichText => ElementKind::RichText,
        K::Other => match p.kept {
            Some(KeptKind::Gallery { .. }) => ElementKind::BuildingBlockGallery,
            Some(KeptKind::BuildingBlock { .. }) => ElementKind::BuildingBlock,
            Some(KeptKind::Citation) => ElementKind::Citation,
            Some(KeptKind::Bibliography) => ElementKind::Bibliography,
            Some(KeptKind::Equation) => ElementKind::Equation,
            None => ElementKind::RichText,
        },
    }
}

/// A content control's look and options, from its properties as kept.
pub fn control_info(p: &crate::docx::ControlProps) -> crate::api::state::ControlInfo {
    let appearance = match p.appearance {
        Some(crate::docx::ControlAppearance::Tags) => crate::api::value::ControlAppearance::Tags,
        Some(crate::docx::ControlAppearance::Hidden) => crate::api::value::ControlAppearance::Hidden,
        _ => crate::api::value::ControlAppearance::BoundingBox,
    };
    let mut info = crate::api::state::ControlInfo {
        appearance,
        color: p.color.flatten().map(Color),
        temporary: p.temporary,
        style: p.run.as_ref().and_then(|r| r.props.style.as_deref()).map(|s| StyleId(s.to_string())),
        ..Default::default()
    };
    match &p.kind {
        crate::docx::ControlKind::Checkbox { checked, unchecked, .. } => {
            (info.checked_symbol, info.unchecked_symbol) = (Some(checked.to_string()), Some(unchecked.to_string()));
        }
        crate::docx::ControlKind::RepeatingSection { title, fixed } => {
            (info.section_title, info.allow_insert_delete) = (title.clone(), Some(!fixed));
        }
        crate::docx::ControlKind::Date { .. } => {
            if let Some(d) = super::merge::date_settings(p) {
                info.calendar = Some(crate::api::value::CalendarType::of_val(&d.calendar).unwrap_or_default());
                info.store_as = Some(d.storage.into());
            }
        }
        crate::docx::ControlKind::Other => {
            if let Some(crate::docx::KeptKind::Gallery { gallery, category } | crate::docx::KeptKind::BuildingBlock { gallery, category, .. }) =
                &p.kept
            {
                (info.gallery, info.category) = (gallery.clone(), category.clone());
            }
        }
        _ => {}
    }
    info
}

#[cfg(test)]
mod underline_tests {
    use super::*;

    #[test]
    fn every_underline_style_reads_back_as_itself() {
        let all: Vec<UnderlineStyle> = serde_json::from_str(
            r#"["none","single","words","double","thick","dotted","dottedHeavy","dash","dashedHeavy","dashLong","dashLongHeavy",
                "dotDash","dashDotHeavy","dotDotDash","dashDotDotHeavy","wave","wavyHeavy","wavyDouble"]"#,
        )
        .expect("every w:u value is a style");
        let doc = crate::docx::read(include_bytes!("../../tests/fixtures/render/underlines.docx")).expect("reads");
        let Some(crate::docx::Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let base = p.inlines.iter().find_map(crate::docx::Inline::run).expect("a run");
        for u in all {
            let value = serde_json::to_value(u).expect("serialises");
            let model = crate::docx::line_style::underline(value.as_str().expect("a string"));
            let r = RunStyle { underline: Some(model).filter(|u| *u != Underline::None), ..(**base).clone() };
            assert_eq!(underline_of(&r), u, "{value}");
            assert_eq!(crate::editing::commands::underline(u), model);
        }
    }
}
