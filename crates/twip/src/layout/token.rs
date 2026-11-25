//! Break tokens

use std::rc::Rc;

use crate::docx::{NoteKind, ParaStyle};

use super::story::StoryUnit;

#[derive(Clone, Debug, Default)]
pub enum At {
    #[default]
    Start,
    /// A paragraph's piece, and while hidden text shows the joined paragraph's part it starts ([`super::space::ParaSpace::part`]).
    Piece(usize, usize),
    Row(usize),
    RowPart {
        row: usize,
        rest: Rc<Vec<Vec<StoryUnit>>>,
    },
}

impl PartialEq for At {
    fn eq(&self, o: &Self) -> bool {
        match (self, o) {
            (At::Start, At::Start) => true,
            (At::Piece(a, p), At::Piece(b, q)) => (a, p) == (b, q),
            (At::Row(a), At::Row(b)) => a == b,
            (At::RowPart { row: a, rest: x }, At::RowPart { row: b, rest: y }) => a == b && Rc::ptr_eq(x, y),
            _ => false,
        }
    }
}

impl At {
    pub fn index(&self) -> usize {
        match self {
            At::Start => 0,
            At::Piece(i, _) | At::Row(i) => *i,
            At::RowPart { row, .. } => *row,
        }
    }

    pub fn at_start(&self) -> bool {
        self.index() == 0 && !matches!(self, At::RowPart { .. } | At::Piece(_, 1..))
    }

    /// The joined paragraph's shown part a paragraph goes on in.
    pub fn part(&self) -> usize {
        match self {
            At::Piece(_, p) => *p,
            _ => 0,
        }
    }

    pub fn rest(&self) -> Option<&Rc<Vec<Vec<StoryUnit>>>> {
        match self {
            At::RowPart { rest, .. } => Some(rest),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlowToken {
    pub block: usize,
    pub at: At,
}

impl FlowToken {
    pub fn next_block(&mut self) {
        self.block += 1;
        self.at = At::Start;
    }
}

#[derive(Clone, Debug)]
pub struct FlowState {
    pub prev: Option<Rc<ParaStyle>>,
    pub prev_after: f64,
    pub keep_top: bool,
    pub first_para: bool,
    pub note_rest: Vec<NotePart>,
    pub line_no: i64,
    pub break_before: f64,
    pub balanced_after: f64,
    /// Compat 15: the footnote area of the earlier sections on the page, drawn after the page's last text, and
    /// whether it goes beneath the text.
    pub page_notes: Option<(NoteArea, bool)>,
    /// A vertical section's table split across pages: where across the turned page its first part was put.
    pub upright_at: Option<f64>,
    /// A table split across pages that went beside floats: how far right of the column its first part was put.
    pub beside_at: Option<f64>,
    /// The rest of a floating table the text after went on beside: it opens the next column.
    pub table_rest: Option<FlowToken>,
    /// The paragraph going on from the last column broke there between letters rather than at a hyphen.
    pub letter_break: bool,
    /// The leading frames of a paragraph that stayed on the last page when the paragraph went on to this one.
    pub frames_left: Vec<(usize, usize, usize)>,
}

impl Default for FlowState {
    fn default() -> Self {
        FlowState {
            prev: None,
            prev_after: 0.0,
            keep_top: true,
            first_para: true,
            note_rest: Vec::new(),
            line_no: 0,
            break_before: 0.0,
            balanced_after: 0.0,
            page_notes: None,
            upright_at: None,
            beside_at: None,
            table_rest: None,
            letter_break: false,
            frames_left: Vec::new(),
        }
    }
}

impl PartialEq for FlowState {
    fn eq(&self, o: &Self) -> bool {
        let FlowState {
            prev,
            prev_after,
            keep_top,
            first_para,
            note_rest,
            line_no,
            break_before,
            balanced_after,
            page_notes,
            upright_at,
            beside_at,
            table_rest,
            letter_break,
            frames_left,
        } = self;
        let style = match (prev, &o.prev) {
            (None, None) => true,
            (Some(x), Some(y)) => Rc::ptr_eq(x, y) || x.layout_eq(y),
            _ => false,
        };
        style
            && *prev_after == o.prev_after
            && *keep_top == o.keep_top
            && *first_para == o.first_para
            && note_rest.is_empty()
            && o.note_rest.is_empty()
            && page_notes.is_none()
            && o.page_notes.is_none()
            && *line_no == o.line_no
            && *break_before == o.break_before
            && *balanced_after == o.balanced_after
            && *upright_at == o.upright_at
            && *beside_at == o.beside_at
            && table_rest.is_none()
            && o.table_rest.is_none()
            && *letter_break == o.letter_break
            && *frames_left == o.frames_left
    }
}

#[derive(Clone, Debug)]
pub struct NotePart {
    pub kind: NoteKind,
    pub id: i64,
    pub units: Vec<StoryUnit>,
}

impl NotePart {
    pub fn height(&self) -> f64 {
        self.units.last().map_or(0.0, |u| u.top + u.height)
    }
}

#[derive(Clone, Default, Debug)]
pub struct NoteArea {
    pub parts: Vec<NotePart>,
    /// How many of `parts` continue notes from an earlier column or page.
    pub carried: usize,
    pub height: f64,
    pub sep: f64,
    pub cont: bool,
    pub shared: Option<f64>,
    /// The area's columns (x, width), those of the section holding the page's first reference; empty for the
    /// column it is filled in.
    pub cols: Vec<(f64, f64)>,
    pub below: bool,
    /// Room kept clear above the notes, down from where a note that goes on on the next page was referenced.
    pub kept: f64,
    /// A note in the area goes on on the next page.
    pub cut: bool,
    /// Room held at the bottom for the continuation notice: compat ≤ 14 on every area, compat 15 once a note is cut.
    pub notice: f64,
    /// The area draws the continuation notice under its cut note.
    pub noticed: bool,
}

impl NoteArea {
    pub fn total(&self) -> f64 {
        if self.below {
            return 0.0;
        }
        self.sep + self.shared.map_or(self.height, |r| self.height.max(r)) + self.kept + self.notice
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageToken {
    pub section: usize,
    pub flow: FlowToken,
    pub state: FlowState,
}
