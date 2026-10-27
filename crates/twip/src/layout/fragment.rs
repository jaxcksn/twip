//! Immutable layout results. Offsets are relative to the parent; child order is flow order; child and item lists are
//! boxed slices. No fragment is mutated after construction: builders ([`BoxBuilder`]) are the only mutable form, and
//! finishing one freezes it. Leaf items are layout primitives only, no renderer types.

use std::rc::Rc;

use crate::docx::NoteKind;
use crate::font::Face;

use crate::docx::VMerge;

use super::ids::{FloatId, FragmentId, ParaId, TableId, Unit};

/// A run of glyphs of one face, size, baseline and colour on one line.
#[derive(Clone, Debug)]
pub struct GlyphRun {
    pub face: Rc<Face>,
    pub size: f64,
    /// Baseline y.
    pub baseline: f64,
    /// The run's origin x. A run without glyphs marks an empty line's caret position.
    pub x: f64,
    /// Glyph ids and origins.
    pub glyphs: Box<[(u32, f64)]>,
    /// The characters drawn, each at its cluster's origin, with its source: `Some` for text and atoms, `None` for
    /// generated text (list numbers and suffixes, tab leaders, line numbers).
    pub chars: Box<[(char, f64, Option<Unit>)]>,
    pub colour: Option<u32>,
    /// The paragraph drawn; `None` for text that belongs to no paragraph (tab leaders, line numbers).
    pub para: Option<ParaId>,
}

/// An index into [`crate::docx::Document::images`] and the generation of the image data it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey {
    /// `None`: a picture without image data (drawn as a placeholder frame).
    pub index: Option<usize>,
    pub generation: u32,
}

#[derive(Clone, Debug)]
pub enum Item {
    Glyphs(GlyphRun),
    /// A filled rectangle (borders, rules, shading, underlines): x0, y0, x1, y1, and an RGB colour.
    Rect {
        rect: [f64; 4],
        colour: u32,
    },
    Image {
        rect: [f64; 4],
        key: ImageKey,
    },
    /// A table cell edge without a border: a dashed line on screen when gridlines show; not printed.
    Gridline {
        line: [f64; 4],
    },
}

impl Item {
    /// The item's top edge (for vertical alignment's "both" distribution, SETUP-1).
    pub fn top(&self) -> f64 {
        match self {
            Item::Glyphs(r) => r.baseline,
            Item::Rect { rect, .. } | Item::Image { rect, .. } => rect[1],
            Item::Gridline { line } => line[1],
        }
    }
}

/// One drawn line: its items relative to the line's top (x in its drawing origin's coordinates, see
/// [crate::layout::space::ParaSpace::x0]). Shared by every fragment that shows it and by the cache that made it, so
/// geometry is never duplicated.
#[derive(Debug)]
pub struct LineFragment {
    pub items: Box<[Item]>,
    /// The line's pitch and baseline (from its top).
    pub pitch: f64,
    pub baseline: f64,
    /// Units drawn without characters (inline pictures, tabs, breaks): x from and to, the unit, and whether it is a
    /// break (a line ending in a break ends before it), in the items' coordinates. Editing places caret stops at them;
    /// painting ignores them.
    pub atoms: Box<[(f64, f64, Unit, bool)]>,
}

/// A line placed in its parent.
#[derive(Clone, Debug)]
pub struct PlacedLine {
    pub x: f64,
    pub y: f64,
    pub line: Rc<LineFragment>,
}

/// The lines of one paragraph in one column (a paragraph split across columns has one fragment per column).
#[derive(Clone, Debug)]
pub struct ParaFragment {
    pub para: ParaId,
    pub lines: Box<[PlacedLine]>,
    /// Line numbers drawn beside these lines (LNUM-1), in the same coordinates as `lines`, one item group per label
    /// (its runs share one line); artefacts.
    pub line_numbers: Box<[Box<[Item]>]>,
}

/// A table cell's part on a page: its content's lines, in the row's coordinates.
#[derive(Clone, Debug)]
pub struct CellFragment {
    /// The cell's place among the row's laid-out cells.
    pub cell: usize,
    pub content: Box<[Child]>,
    /// Where the cell is, for editing: its index in the model row, its grid columns,
    /// its box on this page part, the clip line of an exact row, its vertical merge state, the units an exact row hides
    /// (TABLE-34), and the caret place of a closing paragraph that takes no space (TABLE-26).
    pub place: CellPlace,
}

#[derive(Clone, Debug)]
pub struct CellPlace {
    pub index: usize,
    pub col: usize,
    pub span: usize,
    /// x0, y0, x1, y1 in the row's coordinates; a merge's first cell covers the region's rows on this page part.
    pub rect: [f64; 4],
    pub clip: Option<f64>,
    pub v_merge: VMerge,
    /// Units of the cell's paragraphs hidden by the row's exact height (TABLE-34): paragraph, first unit, end unit.
    pub hidden: Box<[(ParaId, Unit, Unit)]>,
    /// The cell's closing empty paragraph after a nested table, which takes no space (TABLE-26): its paragraph
    /// and caret rectangle (x, top, height), never painted.
    pub closing: Option<(ParaId, [f64; 3])>,
}

impl CellPlace {
    /// The place moved down by `dy` (a row shifted by vertical alignment, SETUP-1).
    pub fn moved_down(&self, dy: f64) -> CellPlace {
        let [x0, y0, x1, y1] = self.rect;
        CellPlace {
            rect: [x0, y0 + dy, x1, y1 + dy],
            clip: self.clip.map(|c| c + dy),
            closing: self.closing.map(|(p, [x, y, h])| (p, [x, y + dy, h])),
            hidden: self.hidden.clone(),
            ..*self
        }
    }
}

/// One table row (or the part of a split row) on a page.
#[derive(Clone, Debug)]
pub struct RowFragment {
    pub row: usize,
    /// The part of the row on this page (x0, y0, x1, y1) and the row's top, in the table's coordinates.
    pub rect: [f64; 4],
    pub top: f64,
    /// Column borders crossing the row: x and grid boundary index (table handles, border dragging).
    pub edges: Box<[(f64, usize)]>,
    /// Cell shading, painted before everything else on the page (the background phase).
    pub background: Box<[Item]>,
    pub cells: Box<[CellFragment]>,
    /// Borders and gridlines, painted over the cells' content.
    pub borders: Box<[Item]>,
    /// A header row repeated at the top of a continuation page (TABLE-46): drawn, but an artefact for the semantic
    /// traversal (its text belongs to the first occurrence).
    pub repeat: bool,
}

#[derive(Clone, Debug)]
pub struct TableFragment {
    pub table: TableId,
    pub rows: Box<[RowFragment]>,
}

/// What a box holds.
#[derive(Clone, Debug, PartialEq)]
pub enum BoxKind {
    /// A section's body on a page.
    Body,
    Column,
    /// A note area: a column's, or the compat-15 shared stream (COL-3).
    Notes,
    /// One note (or note part) in a note area.
    Note {
        kind: NoteKind,
        id: i64,
    },
    /// A note separator (NOTE-3).
    Separator,
    /// A floating picture or text box, anchored in `anchor`. Its children are placed absolutely; `rect` is the placed
    /// box (page coordinates), for hit testing.
    Float {
        anchor: ParaId,
        float: FloatId,
        rect: [f64; 4],
    },
    /// A header or footer: the story part it shows.
    Header {
        story: String,
    },
    Footer {
        story: String,
    },
}

#[derive(Clone, Debug)]
pub enum Child {
    Box(BoxFragment),
    Para(ParaFragment),
    Table(TableFragment),
    /// A placed line that belongs to no paragraph fragment (stories laid out as units: cells, notes, headers).
    Line {
        para: Option<ParaId>,
        placed: PlacedLine,
    },
    /// Loose items: column separators, picture frames, text box outlines, line numbers.
    Items(Box<[Item]>),
    /// Content laid out once and placed at an offset (a story unit: a cell's, a note's or a header's line or row).
    Group {
        offset: (f64, f64),
        children: Rc<[Child]>,
    },
}

/// A box: an offset in its parent, a size, and children in flow order.
#[derive(Clone, Debug)]
pub struct BoxFragment {
    pub kind: BoxKind,
    /// Set on body boxes (page and body identities are what consumers key per-page data by).
    pub id: Option<FragmentId>,
    pub offset: (f64, f64),
    pub size: (f64, f64),
    pub children: Box<[Child]>,
}

/// A laid-out page.
#[derive(Debug)]
pub struct PageFragment {
    pub id: FragmentId,
    pub w: f64,
    pub h: f64,
    /// The first section's body top and bottom on this page (HDR-4 to HDR-6), in page coordinates.
    pub body_area: Option<(f64, f64)>,
    /// Bodies (shared with page records: a reused body is the same `Rc`), headers and footers.
    pub children: Box<[PageChild]>,
}

#[derive(Clone, Debug)]
pub enum PageChild {
    /// A section's body: `Rc` because page records and the baseline share it, and reuse at another page index
    /// keeps it.
    Body(Rc<BoxFragment>),
    Header(BoxFragment),
    Footer(BoxFragment),
}

/// The mutable form of a box under construction. Breakpoints truncate it back to an earlier length; nothing else
/// changes children once pushed.
#[derive(Default)]
pub struct BoxBuilder {
    children: Vec<Child>,
}

impl BoxBuilder {
    pub fn new() -> BoxBuilder {
        BoxBuilder::default()
    }

    pub fn push(&mut self, c: Child) {
        self.children.push(c);
    }

    pub fn len(&self) -> usize {
        self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Drops children pushed after `len` (a rewound keep chain).
    pub fn truncate(&mut self, len: usize) {
        self.children.truncate(len);
    }

    pub fn finish(self, kind: BoxKind, offset: (f64, f64), size: (f64, f64)) -> BoxFragment {
        BoxFragment { kind, id: None, offset, size, children: self.children.into_boxed_slice() }
    }
}
