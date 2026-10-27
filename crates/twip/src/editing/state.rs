//! What the command surfaces show: the formatting at the selection, the
//! context (table, object, story) and what can be undone. One query per reply, O(selection).

use std::rc::Rc;

use crate::docx::{ParaStyle, Run};
use crate::layout::TableId;

use super::Editor;
use super::format::Toggle;
use super::position::StoryId;
use super::selection::{ObjectRef, Selection};

/// What is selected besides text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Picture,
    Float,
    TextBox,
}

/// The state the command bar, menus and dialogs read.
#[derive(Clone, Debug)]
pub struct FormatState {
    /// The caret's run (what typing takes), or the selection's first.
    pub run: Run,
    /// Bold, italic, underline, strike, subscript, superscript: on for the whole selection.
    pub toggles: [bool; 6],
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
    pub object_alt: Option<std::rc::Rc<str>>,
    /// The focus's row and cell in [`FormatState::table`].
    pub cell: Option<(usize, usize)>,
    pub story: StoryId,
    /// The focus's section (body: its own; other stories: the first).
    pub section: usize,
}

const TOGGLES: [Toggle; 6] = [Toggle::Bold, Toggle::Italic, Toggle::Underline, Toggle::Strike, Toggle::Subscript, Toggle::Superscript];

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
        let list = Some(para).and_then(|p| {
            let (id, ilvl) = p.style.num.filter(|(id, _)| *id != 0)?;
            let r = self.doc.resolver.as_ref()?;
            Some(r.numbering.level(id, ilvl, &r.styles)?.fmt == "bullet")
        });
        let caret_run = focus.and_then(|c| self.run_at(&c));
        // shortcut: toggles look at the first 2000 selected paragraphs; a bigger selection shows the first run's.
        let ranges: Vec<_> =
            if self.selection.is_collapsed() { Vec::new() } else { self.selected_ranges().into_iter().filter(|r| r.1 < r.2).take(2000).collect() };
        let runs: Vec<Run> = ranges.iter().flat_map(|&r| self.runs_in(r)).collect();
        let run = runs.first().cloned().or(caret_run).unwrap_or(mark);
        let toggles = TOGGLES.map(|t| if runs.is_empty() { t.on(&run) } else { runs.iter().all(|r| t.on(r)) });
        let level = focus.and_then(|p| self.index.chain_of(&p).last().copied());
        let table = match self.selection {
            Selection::Cells { table, .. } => Some(table),
            _ => level.map(|l| l.table),
        };
        let cell = level.filter(|l| Some(l.table) == table).map(|l| (l.row, l.cell));
        let object = match self.selection {
            Selection::Object(ObjectRef::Inline { .. }) => Some(ObjectKind::Picture),
            Selection::Object(ObjectRef::Floating(f)) => {
                Some(if self.index.text_box(&self.doc, f).is_some() { ObjectKind::TextBox } else { ObjectKind::Float })
            }
            _ => None,
        };
        let object_look = match self.selection {
            Selection::Object(ObjectRef::Inline { para, offset }) => {
                match self.index.para(&self.doc, para).and_then(|p| super::stories::inline_at(p, offset)) {
                    Some(crate::docx::Inline::Picture(p, _)) => Some((p.w, p.h, p.alt.clone())),
                    _ => None,
                }
            }
            Selection::Object(ObjectRef::Floating(f)) => self.index.float_anchor(f).and_then(|a| self.index.para(&self.doc, a)).and_then(|p| {
                p.inlines.iter().find_map(|i| match i {
                    crate::docx::Inline::Float(fl, _) if crate::layout::FloatId(fl.id) == f => Some((fl.w, fl.h, fl.alt.clone())),
                    _ => None,
                })
            }),
            _ => None,
        };
        let object_size = object_look.as_ref().map(|l| (l.0, l.1));
        let object_alt = object_look.map(|l| l.2);
        let story = focus.map_or(self.editing(), |p| p.story);
        let section =
            para_id.and_then(|p| self.index.para_slot(p)).and_then(|s| self.index.top_block(s.container, s.block)).map_or(0, |(sec, _, _)| sec);
        Some(FormatState {
            run,
            toggles,
            para: style,
            list,
            has_selection: !self.selection.is_collapsed(),
            can_undo: self.history.undo_depth() > 0,
            can_redo: self.history.redo_depth() > 0,
            table,
            object,
            object_size,
            object_alt,
            cell,
            story,
            section,
        })
    }
}
