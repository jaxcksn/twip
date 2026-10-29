//! What application controls show: the formatting at the selection with mixed values explicit, the context (table,
//! object, story), and each command's availability and value. One pass over the selection per state build.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::api::catalog::catalog;
use crate::api::command::{Command, CommandId, ParagraphSpacing};
use crate::api::state::{
    BorderState, CharacterFormatting, ColumnLayout, CommandState, CommandValue, ElementContext, ElementKind, FlowState, HeaderFooterInfo,
    HeaderFooterKind, HeaderFooterType, IndentState, LinkInfo, ListState, ObjectSelection, PageFormatting, PageMargins, PageSize,
    ParagraphFormatting, TableSelection, TableStyleOptions, Tri,
};
use crate::api::value::{
    Alignment, AutoOr, CellAlignment, Color, HeightRule, HighlightColor, HorizontalAlignment, LineSpacing, NoneOr, ObjectWrap, Orientation, Points,
    RowHeight, StyleId, TableWidth, UnderlineStyle,
};
use crate::docx::props::{Jc, LineRule, VertAlign};
use crate::docx::{ParaStyle, Run, RunStyle};
use crate::layout::{ParaId, TableId};

use super::Editor;
use super::keymap::{Bound, KeyContext, Mods, Toggle};
use super::position::StoryId;
use super::selection::{ObjectRef, Selection};

/// What is selected besides text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Picture,
    Float,
    TextBox,
}

/// The selected picture or text box.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedObject {
    pub kind: ObjectKind,
    pub width: f64,
    pub height: f64,
    pub alt: Rc<str>,
    /// A float's `wp:wrap*` (`square`, `tight`, `through`, `topAndBottom`, `none`) and whether it is behind the text.
    pub wrap: Option<(String, bool)>,
    pub locked: Option<bool>,
}

/// The state the toolbars, menus and dialogs read.
#[derive(Clone, Debug)]
pub struct FormatState {
    /// The caret's run (what typing takes), or the selection's first.
    pub run: Run,
    /// The selection's character formatting: uniform or mixed.
    pub character: CharacterFormatting,
    /// The selected paragraphs' formatting: uniform or mixed.
    pub paragraph: ParagraphFormatting,
    /// The focus paragraph's properties.
    pub para: Rc<ParaStyle>,
    /// The focus paragraph's list: `Some(true)` bullets, `Some(false)` numbering.
    pub list: Option<bool>,
    pub has_selection: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The innermost table holding the focus, or the table of a cell block.
    pub table: Option<TableId>,
    pub object: Option<ObjectKind>,
    /// The selected object's width and height, points, and its alternative text.
    pub object_size: Option<(f64, f64)>,
    pub object_alt: Option<Rc<str>>,
    /// The focus's row and cell in [`FormatState::table`].
    pub cell: Option<(usize, usize)>,
    pub story: StoryId,
    /// The focus's section (body: its own; other stories: the first).
    pub section: usize,
}

/// `Uniform` when every value is the same, `Mixed` when two differ, `Unavailable` with none.
fn fold<T: PartialEq>(values: impl IntoIterator<Item = T>) -> Tri<T> {
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
    match r.underline.as_deref() {
        None | Some("none") => UnderlineStyle::None,
        Some("words") => UnderlineStyle::Words,
        Some("double") => UnderlineStyle::Double,
        Some("thick") => UnderlineStyle::Thick,
        Some("dotted") => UnderlineStyle::Dotted,
        Some("dash") => UnderlineStyle::Dash,
        Some("dotDash") => UnderlineStyle::DotDash,
        Some("wave") => UnderlineStyle::Wave,
        Some(_) => UnderlineStyle::Single,
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
    let named = |rgb: u32| HIGHLIGHTS.into_iter().find(|h| crate::docx::props::highlight(super::commands::highlight_value(*h)) == Some(rgb));
    match r.highlight.and_then(named) {
        Some(h) => NoneOr::Value(h),
        None => NoneOr::None,
    }
}

fn character(runs: &[&RunStyle]) -> CharacterFormatting {
    let tri = |f: &dyn Fn(&RunStyle) -> bool| fold(runs.iter().map(|r| f(r)));
    CharacterFormatting {
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
    }
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

fn paragraph(paras: &[(&ParaStyle, Option<bool>)]) -> ParagraphFormatting {
    let tri = |f: &dyn Fn(&ParaStyle) -> bool| fold(paras.iter().map(|(p, _)| f(p)));
    let pt = |f: &dyn Fn(&ParaStyle) -> f64| fold(paras.iter().map(|(p, _)| Points(f(p))));
    ParagraphFormatting {
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
        },
        list: fold(paras.iter().map(|(_, l)| match l {
            Some(true) => ListState::Bullet,
            Some(false) => ListState::Numbered,
            None => ListState::None,
        })),
        list_level: fold(paras.iter().filter(|(_, l)| l.is_some()).filter_map(|(p, _)| p.num.map(|(_, l)| l as u32))),
        flow: FlowState {
            keep_with_next: tri(&|p| p.keep_next),
            keep_lines_together: tri(&|p| p.keep_lines),
            page_break_before: tri(&|p| p.page_break_before),
            widow_control: tri(&|p| p.widow_control),
        },
        borders: fold(paras.iter().map(|(p, _)| borders_of(p))),
        shading: fold(paras.iter().map(|(p, _)| p.shading.map_or(NoneOr::None, |c| NoneOr::Value(Color(c))))),
    }
}

/// What a state build knows besides the editor: the session's settings.
#[derive(Clone, Copy, Debug, Default)]
pub struct StateFlags {
    pub read_only: bool,
}

impl Editor {
    /// `None` when nothing holds a paragraph (no document content).
    pub fn format_state(&mut self) -> Option<FormatState> {
        let focus = self.selection.focus();
        let para_id = focus.and_then(|p| p.para_offset()).map(|(p, _)| p).or_else(|| match self.selection {
            Selection::Object(ObjectRef::Inline { para, .. }) => Some(para),
            Selection::Object(ObjectRef::Floating(f)) => self.index.float_anchor(f),
            _ => None,
        });
        let para = self.index.para(&self.doc, para_id?)?;
        let (mark, style) = (para.mark.clone(), para.style.clone());
        let list = self.para_list(para_id?);
        let caret_run = focus.and_then(|c| self.run_at(&c));
        // shortcut: values look at the first 2000 selected paragraphs; a bigger selection shows theirs.
        let ranges: Vec<(ParaId, crate::layout::Unit, crate::layout::Unit)> =
            if self.selection.is_collapsed() { Vec::new() } else { self.selected_ranges().into_iter().take(2000).collect() };
        let runs: Vec<Run> = ranges.iter().filter(|r| r.1 < r.2).flat_map(|&r| self.runs_in(r)).collect();
        let run = runs.first().cloned().or(caret_run).unwrap_or(mark);
        let character = if runs.is_empty() { self::character(&[&run]) } else { self::character(&runs.iter().map(|r| &**r).collect::<Vec<_>>()) };
        let mut paras: Vec<ParaId> = ranges.iter().map(|r| r.0).collect();
        paras.dedup();
        if paras.is_empty() {
            paras.push(para_id?);
        }
        let styles: Vec<(Rc<ParaStyle>, Option<bool>)> =
            paras.iter().filter_map(|&p| Some((self.index.para(&self.doc, p)?.style.clone(), self.para_list(p)))).collect();
        let paragraph = self::paragraph(&styles.iter().map(|(s, l)| (&**s, *l)).collect::<Vec<_>>());
        let level = focus.and_then(|p| self.index.chain_of(&p).last().copied());
        let table = match self.selection {
            Selection::Cells { table, .. } => Some(table),
            _ => level.map(|l| l.table),
        };
        let cell = level.filter(|l| Some(l.table) == table).map(|l| (l.row, l.cell));
        let object = self.selected_object();
        let story = focus.map_or(self.editing(), |p| p.story);
        let section =
            para_id.and_then(|p| self.index.para_slot(p)).and_then(|s| self.index.top_block(s.container, s.block)).map_or(0, |(sec, _, _)| sec);
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

    /// The selected picture or text box.
    pub fn selected_object(&self) -> Option<SelectedObject> {
        match self.selection {
            Selection::Object(ObjectRef::Inline { para, offset }) => {
                match self.index.para(&self.doc, para).and_then(|p| super::stories::inline_at(p, offset)) {
                    Some(crate::docx::Inline::Picture(p, _)) => {
                        Some(SelectedObject { kind: ObjectKind::Picture, width: p.w, height: p.h, alt: p.alt.clone(), wrap: None, locked: None })
                    }
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
                        wrap: Some((fl.wrap.clone(), fl.behind)),
                        locked: Some(fl.locked),
                    }),
                    _ => None,
                })
            }
            _ => None,
        }
    }

    /// The command a key press runs now (the keymap in the caret's context, a setter's value from the selection), if
    /// any: what the host claims the key for.
    pub fn key_command(&mut self, code: &str, mods: Mods) -> Option<Command> {
        let cx = KeyContext {
            in_table: self.focus_chain_depth() > 0 || matches!(self.selection, Selection::Cells { .. }),
            list_start: self.list_item_start().is_some(),
            float: matches!(self.selection, Selection::Object(ObjectRef::Floating(_))),
            mac: self.mac,
        };
        match super::keymap::command(code, mods, cx)? {
            Bound::Run(c) => Some(c),
            Bound::Toggle(t) => self.toggled(t),
        }
    }

    /// A toggle key's setter, with the value it sets now: on unless the whole selection has it.
    fn toggled(&mut self, t: Toggle) -> Option<Command> {
        if let Toggle::TrackChanges = t {
            return Some(Command::SetTrackChanges(!self.tracking()));
        }
        if let Toggle::FieldCodes { all } = t {
            return self.field_codes_toggle(all).map(Command::ShowFieldCodes);
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
            Toggle::Subscript => Command::SetSubscript(on(&c.subscript)),
            Toggle::Superscript => Command::SetSuperscript(on(&c.superscript)),
            Toggle::SmallCaps => Command::SetSmallCaps(on(&c.small_caps)),
            Toggle::AllCaps => Command::SetAllCaps(on(&c.all_caps)),
            Toggle::Hidden => Command::SetHidden(on(&c.hidden)),
            Toggle::Align(a) => Command::SetAlignment(if f.paragraph.alignment == Tri::Uniform(a) { Alignment::Left } else { a }),
            Toggle::SpaceBefore => {
                let some = f.para.space_before > 0.0;
                Command::SetParagraphSpacing(ParagraphSpacing { before: Some(AutoOr::Value(Points(if some { 0.0 } else { 12.0 }))), after: None })
            }
            Toggle::TrackChanges | Toggle::FieldCodes { .. } => return None,
        })
    }

    /// Each command's availability, toggle state and value, from one format state.
    pub fn command_states(&mut self, f: Option<&FormatState>, flags: StateFlags) -> BTreeMap<CommandId, CommandState> {
        use CommandId as C;
        let editable = !flags.read_only;
        let formatted = editable && f.is_some();
        let selected = !self.selection.is_collapsed();
        let in_table = f.is_some_and(|f| f.table.is_some());
        let story = f.map_or(StoryId::Body, |f| f.story);
        let body_text = editable && story == StoryId::Body && !in_table;
        let object = f.and_then(|f| f.object);
        let floating = matches!(object, Some(ObjectKind::Float | ObjectKind::TextBox));
        let in_list = f.is_some_and(|f| f.list.is_some());
        let header_footer = matches!(story, StoryId::HeaderFooter(_));
        let can_merge = matches!(self.selection, Selection::Cells { anchor, focus, .. } if anchor != focus);
        let link = self.link_here().is_some();
        let comments = !self.doc.comments.by_id.is_empty();
        let back = !self.edit_marks.is_empty();
        let repeat = f.is_some_and(|f| f.can_redo) || self.repeat.is_some();
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
                // The system clipboard is the host's to read: a paste can go wherever text can.
                (C::Paste, _) => (editable, (None, None)),
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
                (C::SetBold, Some(f)) => (formatted, toggle(f.character.bold)),
                (C::SetItalic, Some(f)) => (formatted, toggle(f.character.italic)),
                (C::SetStrikethrough, Some(f)) => (formatted, toggle(f.character.strikethrough)),
                (C::SetDoubleStrikethrough, Some(f)) => (formatted, toggle(f.character.double_strikethrough)),
                (C::SetSubscript, Some(f)) => (formatted, toggle(f.character.subscript)),
                (C::SetSuperscript, Some(f)) => (formatted, toggle(f.character.superscript)),
                (C::SetSmallCaps, Some(f)) => (formatted, toggle(f.character.small_caps)),
                (C::SetAllCaps, Some(f)) => (formatted, toggle(f.character.all_caps)),
                (C::SetHidden, Some(f)) => (formatted, toggle(f.character.hidden)),
                (C::SetUnderline, Some(f)) => (formatted, (None, Some(CommandValue::Underline(f.character.underline)))),
                (C::SetFont, Some(f)) => (formatted, (None, Some(CommandValue::Text(f.character.font.clone())))),
                (C::SetFontSize, Some(f)) => (formatted, (None, Some(CommandValue::Length(f.character.size)))),
                (C::SetFontColor, Some(f)) => (formatted, (None, Some(CommandValue::Color(f.character.color)))),
                (C::SetHighlight, Some(f)) => (formatted, (None, Some(CommandValue::Highlight(f.character.highlight)))),
                (C::SetCharacterSpacing, Some(f)) => (formatted, (None, Some(CommandValue::Length(f.character.character_spacing)))),
                (C::SetCharacterPosition, Some(f)) => (formatted, (None, Some(CommandValue::Length(f.character.position)))),
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
                    | C::DistributeRows
                    | C::DistributeColumns
                    | C::MoveRows
                    | C::AutoFitColumns
                    | C::SetCellAlignment
                    | C::SetCellShading
                    | C::SetCellBorders
                    | C::SetCellMargins
                    | C::SetHeaderRow
                    | C::SetRowHeight
                    | C::SetRowCanBreak
                    | C::SetColumnWidth
                    | C::SetTableWidth
                    | C::SetTableAlignment
                    | C::SetTableIndent
                    | C::SetTableStyle
                    | C::SetTableStyleOptions,
                    _,
                ) => (editable && in_table, (None, None)),
                (C::SetObjectSize | C::SetObjectWrap | C::SetAltText | C::DeleteObject, _) => (editable && object.is_some(), (None, None)),
                (C::ResetObjectSize, _) => (editable && matches!(object, Some(ObjectKind::Picture | ObjectKind::Float)), (None, None)),
                (C::SetAnchorLocked, _) => (editable && floating, (None, None)),
                (C::SetTrackChanges, _) => (editable && !(locked && tracking), (Some(tracking), None)),
                (C::SetMarkupView, _) => (true, (None, Some(CommandValue::MarkupView(self.doc.review.view)))),
                (C::SetActiveComment | C::SelectComment, _) => (comments, (None, None)),
                (C::ReplyToComment | C::EditComment | C::DeleteComment | C::ResolveComment, _) => (editable && comments, (None, None)),
                (C::SetDifferentFirstPage, Some(f)) => {
                    let on = self.doc.sections.get(f.section).is_some_and(|s| s.title_pg);
                    (editable, (Some(on), None))
                }
                (_, None) if info.mutates => (false, (None, None)),
                _ => (editable || !info.mutates, (None, None)),
            };
            out.insert(id, CommandState { enabled, checked, value });
        }
        out
    }

    /// The selection's table: its shape, the current cell and the values the table commands set.
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
            cell_alignment: fold(cells.iter().map(|c| match c.v_align.as_str() {
                "center" => CellAlignment::Center,
                "bottom" => CellAlignment::Bottom,
                _ => CellAlignment::Top,
            })),
            cell_shading: fold(cells.iter().map(|c| c.shading.map_or(NoneOr::None, |s| NoneOr::Value(Color(s))))),
            row_height: fold(rows.iter().map(|r| match &r.height {
                None => AutoOr::Auto,
                Some((h, rule)) => {
                    AutoOr::Value(RowHeight { height: Points(*h), rule: if rule == "exact" { HeightRule::Exact } else { HeightRule::AtLeast } })
                }
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
        })
    }

    /// The selected picture or text box, as the state reports it.
    pub fn object_selection(&self) -> Option<ObjectSelection> {
        let o = self.selected_object()?;
        let wrap = match o.wrap.as_ref().map(|(w, behind)| (w.as_str(), *behind)) {
            None => ObjectWrap::Inline,
            Some(("square", _)) => ObjectWrap::Square,
            Some(("tight", _)) => ObjectWrap::Tight,
            Some(("through", _)) => ObjectWrap::Through,
            Some(("topAndBottom", _)) => ObjectWrap::TopAndBottom,
            Some((_, true)) => ObjectWrap::BehindText,
            Some((_, false)) => ObjectWrap::InFrontOfText,
        };
        Some(ObjectSelection {
            kind: if o.kind == ObjectKind::TextBox { crate::api::state::ObjectKind::TextBox } else { crate::api::state::ObjectKind::Picture },
            wrap,
            width: Points(o.width),
            height: Points(o.height),
            alt_text: o.alt.to_string(),
            decorative: false,
            anchor_locked: o.locked,
        })
    }

    /// The elements the selection is in or covers, innermost first.
    pub fn element_context(&self) -> Vec<ElementContext> {
        let para = self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para(&self.doc, p));
        super::elements::context(&self.doc, &self.index, &self.selection)
            .into_iter()
            .map(|(e, exact)| {
                let id = super::merge::api_id(e);
                match e {
                    crate::docx::ElementId::Field(f) => {
                        let code = para.and_then(|p| {
                            let k = p.inlines.iter().position(|i| matches!(i, crate::docx::Inline::FieldBegin { id, .. } if *id == f))?;
                            let s = crate::docx::fields::span_at(&p.inlines, k)?;
                            Some(crate::docx::fields::code_text(&p.inlines, s).trim().to_string())
                        });
                        ElementContext { id, kind: ElementKind::Field, exact, code, tag: None, title: None }
                    }
                    crate::docx::ElementId::Control(c) => {
                        let props = self.doc.controls.get(&c).map(|k| k.props.clone()).unwrap_or_default();
                        let kind = match props.kind {
                            crate::docx::ControlKind::PlainText { .. } => ElementKind::PlainText,
                            crate::docx::ControlKind::Picture => ElementKind::Picture,
                            crate::docx::ControlKind::Checkbox { .. } => ElementKind::Checkbox,
                            crate::docx::ControlKind::Date { .. } => ElementKind::Date,
                            crate::docx::ControlKind::DropDown if props.kind_xml.as_deref().is_some_and(|x| x.contains("comboBox")) => {
                                ElementKind::ComboBox
                            }
                            crate::docx::ControlKind::DropDown => ElementKind::DropDown,
                            crate::docx::ControlKind::Group => ElementKind::Group,
                            crate::docx::ControlKind::RepeatingSection => ElementKind::RepeatingSection,
                            crate::docx::ControlKind::RepeatingItem => ElementKind::RepeatingItem,
                            crate::docx::ControlKind::RichText | crate::docx::ControlKind::Other => ElementKind::RichText,
                        };
                        ElementContext { id, kind, exact, code: None, tag: props.tag, title: props.alias }
                    }
                }
            })
            .collect()
    }

    /// The link at the selection.
    pub fn link_info(&self) -> Option<LinkInfo> {
        self.link_here().map(|l| LinkInfo { address: l.address, text: l.text })
    }

    /// The header or footer being edited: which, for which pages, its section, the caret's page and its label.
    pub fn header_footer_info(&mut self, page: u32) -> Option<HeaderFooterInfo> {
        let StoryId::HeaderFooter(k) = self.selection.focus()?.story else { return None };
        let name = self.index.part_name(k)?.to_string();
        let (section, kind, variant) = self.doc.sections.iter().enumerate().find_map(|(i, s)| {
            let at = |refs: &crate::docx::HfRefs| refs.iter().position(|r| r.as_deref() == Some(name.as_str()));
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

    /// A section's page setup.
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
            columns: ColumnLayout { count: s.columns.num.max(1) as u32, spacing: Points(s.columns.space), separator: s.columns.separator },
            header_distance: Points(s.header),
            footer_distance: Points(s.footer),
            different_first_page: s.title_pg,
            different_odd_even: self.doc.settings.even_and_odd,
        }
    }
}
