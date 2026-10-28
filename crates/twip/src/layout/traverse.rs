//! The three orders consumers read fragments in, each defined here once.
//!
//! - **Paint**: cell shading (the background phase), then body flow, notes, floats, headers and footers; within a
//!   line, its items in the order the line was drawn (border rects, highlight and shading, glyphs and pictures,
//!   underline and strikethrough).
//! - **Semantic**: document order by paragraph, for tagged PDF, accessibility and tests.
//! - **Spatial**: per page, lines and table rows with their rectangles, for hit testing and the caret.
//!
//! Text pushed entirely below its page (HDR-9) is in no traversal. The predicate is applied per glyph run.

use crate::docx::NoteKind;

use super::fragment::{BoxFragment, BoxKind, CellPlace, Child, GlyphRun, Item, LineFragment, PageChild, PageFragment, PlacedLine, RowFragment};
use super::ids::{FloatId, ParaId, TableId, Unit};

/// An item and where its coordinates are relative to: add `dx`, `dy` for page coordinates. `line` numbers the drawn
/// line the item belongs to within one traversal (a placed line, a line-number label, a group of loose items), so
/// consumers can group glyph runs into lines; 0 for items that belong to none (shading, borders).
#[derive(Clone, Copy)]
pub struct Placed<'a> {
    pub dx: f64,
    pub dy: f64,
    pub item: &'a Item,
    pub line: u32,
}

impl Placed<'_> {
    /// The item in page coordinates.
    pub fn absolute(&self) -> Item {
        translated(self.item, self.dx, self.dy)
    }
}

/// `item` moved by (dx, dy).
pub fn translated(item: &Item, dx: f64, dy: f64) -> Item {
    let r4 = |r: &[f64; 4]| [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy];
    match item {
        Item::Glyphs(r) => Item::Glyphs(GlyphRun {
            face: r.face.clone(),
            size: r.size,
            baseline: r.baseline + dy,
            x: r.x + dx,
            glyphs: r.glyphs.iter().map(|&(g, x)| (g, x + dx)).collect(),
            chars: r.chars.iter().map(|&(c, x, u)| (c, x + dx, u)).collect(),
            colour: r.colour,
            para: r.para,
        }),
        Item::Rect { rect, colour } => Item::Rect { rect: r4(rect), colour: *colour },
        Item::Image { rect, key } => Item::Image { rect: r4(rect), key: *key },
        Item::Gridline { line } => Item::Gridline { line: r4(line) },
    }
}

/// Whether a glyph run at page offset `dy` is entirely below a page of height `h` (HDR-9).
fn below_page(item: &Item, dy: f64, h: f64) -> bool {
    matches!(item, Item::Glyphs(r) if r.baseline + dy - r.size >= h)
}

/// Numbers drawn lines as a traversal meets them.
struct Lines(u32);

impl Lines {
    fn next(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

fn line_items<'a>(p: &'a PlacedLine, dx: f64, dy: f64, lines: &mut Lines, out: &mut dyn FnMut(Placed<'a>)) {
    let line = lines.next();
    for it in p.line.items.iter() {
        out(Placed { dx: dx + p.x, dy: dy + p.y, item: it, line });
    }
}

/// The background phase: cell shading in `children`, skipping the boxes `skip` names (they have their own phase).
fn backgrounds<'a>(children: &'a [Child], dx: f64, dy: f64, skip: &dyn Fn(&BoxKind) -> bool, out: &mut dyn FnMut(Placed<'a>)) {
    for c in children {
        match c {
            Child::Box(b) if skip(&b.kind) => {}
            Child::Box(b) => backgrounds(&b.children, dx + b.offset.0, dy + b.offset.1, skip, out),
            Child::Group { offset, children } => backgrounds(children, dx + offset.0, dy + offset.1, skip, out),
            Child::Table(t) => {
                for r in t.rows.iter() {
                    for it in r.background.iter() {
                        out(Placed { dx, dy, item: it, line: 0 });
                    }
                    for cell in r.cells.iter() {
                        backgrounds(&cell.content, dx, dy, skip, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Everything in `children` but backgrounds, in flow order; floats and note areas are left to their own phases when
/// `skip` says so.
fn flow<'a>(children: &'a [Child], dx: f64, dy: f64, skip: &dyn Fn(&BoxKind) -> bool, lines: &mut Lines, out: &mut dyn FnMut(Placed<'a>)) {
    for c in children {
        match c {
            Child::Box(b) if skip(&b.kind) => {}
            Child::Box(b) => flow(&b.children, dx + b.offset.0, dy + b.offset.1, skip, lines, out),
            Child::Group { offset, children } => flow(children, dx + offset.0, dy + offset.1, skip, lines, out),
            Child::Para(p) => {
                for l in p.lines.iter() {
                    line_items(l, dx, dy, lines, out);
                }
                for label in p.line_numbers.iter() {
                    let line = lines.next();
                    for it in label.iter() {
                        out(Placed { dx, dy, item: it, line });
                    }
                }
            }
            Child::Table(t) => {
                for r in t.rows.iter() {
                    row(r, dx, dy, skip, lines, out);
                }
            }
            Child::Line { placed, .. } => line_items(placed, dx, dy, lines, out),
            Child::Items(items) => {
                let line = lines.next();
                for it in items.iter() {
                    out(Placed { dx, dy, item: it, line });
                }
            }
        }
    }
}

fn row<'a>(r: &'a RowFragment, dx: f64, dy: f64, skip: &dyn Fn(&BoxKind) -> bool, lines: &mut Lines, out: &mut dyn FnMut(Placed<'a>)) {
    for cell in r.cells.iter() {
        flow(&cell.content, dx, dy, skip, lines, out);
    }
    for it in r.borders.iter() {
        out(Placed { dx, dy, item: it, line: 0 });
    }
}

/// The boxes of kind `want` anywhere in `children`, with their parents' offsets.
fn boxes_of<'a>(children: &'a [Child], dx: f64, dy: f64, want: &dyn Fn(&BoxKind) -> bool, out: &mut Vec<(&'a BoxFragment, f64, f64)>) {
    for c in children {
        match c {
            Child::Box(b) if want(&b.kind) => out.push((b, dx, dy)),
            Child::Box(b) => boxes_of(&b.children, dx + b.offset.0, dy + b.offset.1, want, out),
            Child::Group { offset, children } => boxes_of(children, dx + offset.0, dy + offset.1, want, out),
            Child::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| boxes_of(&c.content, dx, dy, want, out)),
            _ => {}
        }
    }
}

fn is_float(k: &BoxKind) -> bool {
    matches!(k, BoxKind::Float { .. })
}

fn is_notes(k: &BoxKind) -> bool {
    matches!(k, BoxKind::Notes)
}

/// The page's items in paint order, each with its offset to page coordinates.
pub fn paint<'a>(page: &'a PageFragment, out: &mut dyn FnMut(Placed<'a>)) {
    let h = page.h;
    let mut visible = |p: Placed<'a>| {
        if !below_page(p.item, p.dy, h) {
            out(p);
        }
    };
    let mut lines = Lines(0);
    let bodies = || page.children.iter().filter_map(|c| if let PageChild::Body(b) = c { Some(&**b) } else { None });
    let neither = |k: &BoxKind| is_float(k) || is_notes(k);
    for b in bodies() {
        backgrounds(&b.children, b.offset.0, b.offset.1, &neither, &mut visible);
    }
    for b in bodies() {
        flow(&b.children, b.offset.0, b.offset.1, &neither, &mut lines, &mut visible);
    }
    for want in [&is_notes as &dyn Fn(&BoxKind) -> bool, &is_float] {
        for b in bodies() {
            let mut found = Vec::new();
            boxes_of(&b.children, b.offset.0, b.offset.1, want, &mut found);
            for (fb, dx, dy) in found {
                backgrounds(&fb.children, dx + fb.offset.0, dy + fb.offset.1, &|_| false, &mut visible);
                flow(&fb.children, dx + fb.offset.0, dy + fb.offset.1, &|_| false, &mut lines, &mut visible);
            }
        }
    }
    for c in page.children.iter() {
        if let PageChild::Header(b) | PageChild::Footer(b) = c {
            backgrounds(&b.children, b.offset.0, b.offset.1, &|_| false, &mut visible);
            flow(&b.children, b.offset.0, b.offset.1, &|_| false, &mut lines, &mut visible);
        }
    }
}
/// The page's items in paint order, in page coordinates.
pub fn paint_items(page: &PageFragment) -> Vec<Item> {
    let mut v = Vec::new();
    paint(page, &mut |p| v.push(p.absolute()));
    v
}

/// A body's text pushed entirely below a page of height `h` (HDR-9), by paragraph: what the traversals leave out.
pub fn excluded_in(body: &BoxFragment, h: f64) -> Vec<(ParaId, std::ops::Range<Unit>)> {
    let mut out = Vec::new();
    let mut each = |p: Placed| {
        if let Item::Glyphs(r) = p.item
            && below_page(p.item, p.dy, h)
            && let Some(para) = r.para
        {
            let units: Vec<Unit> = r.chars.iter().filter_map(|c| c.2).collect();
            if let (Some(lo), Some(hi)) = (units.iter().min(), units.iter().max()) {
                out.push((para, *lo..Unit(hi.0 + 1)));
            }
        }
    };
    flow(&body.children, body.offset.0, body.offset.1, &|_| false, &mut Lines(0), &mut each);
    out
}

/// The glyph runs of a page excluded by HDR-9 (for tests and diagnostics): their paragraph and source units.
pub fn excluded(page: &PageFragment) -> Vec<(Option<ParaId>, Vec<Unit>)> {
    let mut all = Vec::new();
    let mut each = |p: Placed| {
        if let Item::Glyphs(r) = p.item
            && below_page(p.item, p.dy, page.h)
        {
            all.push((r.para, r.chars.iter().filter_map(|c| c.2).collect()));
        }
    };
    // Paint skips them, so walk the flow directly (glyph runs are never in the background phase).
    for c in page.children.iter() {
        match c {
            PageChild::Body(b) => flow(&b.children, b.offset.0, b.offset.1, &|_| false, &mut Lines(0), &mut each),
            PageChild::Header(b) | PageChild::Footer(b) => flow(&b.children, b.offset.0, b.offset.1, &|_| false, &mut Lines(0), &mut each),
        }
    }
    all
}

/// A line on a page, for hit testing and the caret: its rectangle (page coordinates) and the paragraph units it
/// shows (those with a source). A line showing several paragraphs (REV-6) has one per paragraph.
#[derive(Clone, Debug)]
pub struct SpatialLine {
    pub rect: [f64; 4],
    pub para: Option<ParaId>,
    pub units: Option<(Unit, Unit)>,
}

/// A table row on a page: its table, index, rectangle and column edges (page coordinates).
#[derive(Clone, Debug)]
pub struct SpatialRow {
    pub table: TableId,
    pub row: usize,
    pub rect: [f64; 4],
    pub top: f64,
    pub edges: Vec<(f64, usize)>,
}

pub enum Spatial {
    /// The page shows provisional content: positions on it don't resolve.
    Provisional,
    Page {
        lines: Vec<SpatialLine>,
        rows: Vec<SpatialRow>,
    },
}

/// The page's lines and rows in visual order (top to bottom, then left to right).
pub fn spatial(page: &PageFragment, provisional: bool) -> Spatial {
    if provisional {
        return Spatial::Provisional;
    }
    let (mut lines, mut rows) = (Vec::new(), Vec::new());
    fn add(lines: &mut Vec<SpatialLine>, para: Option<ParaId>, p: &PlacedLine, dx: f64, dy: f64, h: f64) {
        {
            let (x, y) = (dx + p.x, dy + p.y);
            // HDR-9: a line pushed entirely below the page isn't on it.
            let mut runs = p.line.items.iter().filter(|it| matches!(it, Item::Glyphs(_))).peekable();
            if runs.peek().is_some() && runs.all(|it| below_page(it, y, h)) {
                return;
            }
            // One entry per paragraph the line shows: several where the view joins paragraphs (REV-6).
            let first = lines.len();
            for it in p.line.items.iter() {
                let Item::Glyphs(r) = it else { continue };
                let q = r.para.or(para);
                let k = match lines[first..].iter().position(|l| l.para == q) {
                    Some(k) => first + k,
                    None => {
                        // The line's first entry starts at its left edge, a later one where its text does.
                        let left = if lines.len() == first { x } else { r.x + x };
                        lines.push(SpatialLine { rect: [left, y, left, y + p.line.pitch], para: q, units: None });
                        lines.len() - 1
                    }
                };
                let l = &mut lines[k];
                for &(_, cx, u) in r.chars.iter() {
                    l.rect[2] = l.rect[2].max(cx + x);
                    if let Some(u) = u {
                        l.units = Some(l.units.map_or((u, u), |(a, b)| (a.min(u), b.max(u))));
                    }
                }
            }
            if lines.len() == first {
                lines.push(SpatialLine { rect: [x, y, x, y + p.line.pitch], para, units: None });
            }
        }
    }
    fn walk(children: &[Child], dx: f64, dy: f64, h: f64, lines: &mut Vec<SpatialLine>, rows: &mut Vec<SpatialRow>) {
        for c in children {
            match c {
                Child::Box(b) => walk(&b.children, dx + b.offset.0, dy + b.offset.1, h, lines, rows),
                Child::Group { offset, children } => walk(children, dx + offset.0, dy + offset.1, h, lines, rows),
                Child::Para(p) => {
                    for l in p.lines.iter() {
                        add(lines, Some(p.para), l, dx, dy, h);
                    }
                }
                Child::Line { para, placed } => add(lines, *para, placed, dx, dy, h),
                Child::Table(t) => {
                    for r in t.rows.iter() {
                        let rect = [r.rect[0] + dx, r.rect[1] + dy, r.rect[2] + dx, r.rect[3] + dy];
                        rows.push(SpatialRow {
                            table: t.table,
                            row: r.row,
                            rect,
                            top: r.top + dy,
                            edges: r.edges.iter().map(|&(x, k)| (x + dx, k)).collect(),
                        });
                        for cell in r.cells.iter() {
                            walk(&cell.content, dx, dy, h, lines, rows);
                        }
                    }
                }
                Child::Items(_) => {}
            }
        }
    }
    for c in page.children.iter() {
        let b = match c {
            PageChild::Body(b) => &**b,
            PageChild::Header(b) | PageChild::Footer(b) => b,
        };
        walk(&b.children, b.offset.0, b.offset.1, page.h, &mut lines, &mut rows);
    }
    lines.sort_by(|a, b| a.rect[1].total_cmp(&b.rect[1]).then(a.rect[0].total_cmp(&b.rect[0])));
    Spatial::Page { lines, rows }
}

/// A page's content by containment, for editing: hit testing, caret places and table targets.
/// Coordinates are the page's.
pub struct SpatialTree<'a> {
    /// Bodies, then the floats and notes met in them, then headers and footers.
    pub stories: Vec<SpatialStory<'a>>,
}

/// One story's part on the page: a section body, a header or footer, a note, or a float (picture or text box).
pub struct SpatialStory<'a> {
    pub story: StoryRef<'a>,
    pub rect: [f64; 4],
    pub nodes: Vec<SpatialNode<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StoryRef<'a> {
    Body,
    Header(&'a str),
    Footer(&'a str),
    Note { kind: NoteKind, id: i64 },
    Float { float: FloatId, anchor: ParaId },
}

pub enum SpatialNode<'a> {
    Line(LineNode<'a>),
    Table(TableNode<'a>),
}

/// A drawn line: its glyph runs are relative to (`x`, `y`), the line's top left. `para` is the paragraph laid out;
/// the glyph runs and atoms name the paragraphs they show, several where the view joins paragraphs (REV-6).
pub struct LineNode<'a> {
    pub para: Option<ParaId>,
    pub x: f64,
    pub y: f64,
    pub line: &'a LineFragment,
}

pub struct TableNode<'a> {
    pub table: TableId,
    pub rows: Vec<RowNode<'a>>,
}

pub struct RowNode<'a> {
    pub row: usize,
    /// The row's part on this page, and the row's top.
    pub rect: [f64; 4],
    pub top: f64,
    /// A header row repeated on a continuation page (TABLE-46): it shows the first occurrence's text.
    pub repeat: bool,
    /// Column borders crossing the row: x and grid boundary index.
    pub edges: Vec<(f64, usize)>,
    pub cells: Vec<CellNode<'a>>,
}

pub struct CellNode<'a> {
    /// Index in the model row, grid columns, merge state and hidden units (TABLE-34).
    pub place: &'a CellPlace,
    /// The cell's box, its clip line, and the closing paragraph's caret place (x, top, height): page coordinates.
    pub rect: [f64; 4],
    pub clip: Option<f64>,
    pub closing: Option<(ParaId, [f64; 3])>,
    pub nodes: Vec<SpatialNode<'a>>,
}

/// The page's [`SpatialTree`]; `None` for a provisional page, whose positions don't resolve.
pub fn spatial_tree(page: &PageFragment, provisional: bool) -> Option<SpatialTree<'_>> {
    if provisional {
        return None;
    }
    let mut w = SpatialWalk { h: page.h, stories: Vec::new() };
    let mut bodies = Vec::new();
    for c in page.children.iter() {
        if let PageChild::Body(b) = c {
            let mut nodes = Vec::new();
            w.children(&b.children, b.offset.0, b.offset.1, &mut nodes);
            let (x, y) = b.offset;
            bodies.push(SpatialStory { story: StoryRef::Body, rect: [x, y, x + b.size.0, y + b.size.1], nodes });
        }
    }
    let floats_and_notes = std::mem::take(&mut w.stories);
    for c in page.children.iter() {
        match c {
            PageChild::Header(b) => {
                if let BoxKind::Header { story } = &b.kind {
                    w.story(StoryRef::Header(story), b, 0.0, 0.0);
                }
            }
            PageChild::Footer(b) => {
                if let BoxKind::Footer { story } = &b.kind {
                    w.story(StoryRef::Footer(story), b, 0.0, 0.0);
                }
            }
            PageChild::Body(_) => {}
        }
    }
    let mut stories = bodies;
    stories.extend(floats_and_notes);
    stories.extend(w.stories);
    Some(SpatialTree { stories })
}

struct SpatialWalk<'a> {
    h: f64,
    stories: Vec<SpatialStory<'a>>,
}

impl<'a> SpatialWalk<'a> {
    fn line(&self, para: Option<ParaId>, p: &'a PlacedLine, dx: f64, dy: f64, out: &mut Vec<SpatialNode<'a>>) {
        let (x, y) = (dx + p.x, dy + p.y);
        // HDR-9: a line pushed entirely below the page isn't on it.
        let mut runs = p.line.items.iter().filter(|it| matches!(it, Item::Glyphs(_))).peekable();
        if runs.peek().is_some() && runs.all(|it| below_page(it, y, self.h)) {
            return;
        }
        out.push(SpatialNode::Line(LineNode { para, x, y, line: &p.line }));
    }

    /// A story's box: the bounds of its lines and rows (header, footer and note boxes place their content
    /// absolutely, so their own offset says nothing).
    fn story(&mut self, story: StoryRef<'a>, b: &'a BoxFragment, dx: f64, dy: f64) {
        let (x, y) = (dx + b.offset.0, dy + b.offset.1);
        let mut nodes = Vec::new();
        self.children(&b.children, x, y, &mut nodes);
        let mut r = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        let mut add = |q: [f64; 4]| r = [r[0].min(q[0]), r[1].min(q[1]), r[2].max(q[2]), r[3].max(q[3])];
        for n in &nodes {
            match n {
                SpatialNode::Line(l) => add([l.x, l.y, l.x + 1.0, l.y + l.line.pitch]),
                SpatialNode::Table(t) => t.rows.iter().for_each(|row| add(row.rect)),
            }
        }
        // As wide as the box, from its leftmost content.
        let rect = if r[0].is_finite() { [r[0], r[1], r[2].max(r[0] + b.size.0), r[3]] } else { [x, y, x + b.size.0, y + b.size.1] };
        self.stories.push(SpatialStory { story, rect, nodes });
    }

    fn children(&mut self, children: &'a [Child], dx: f64, dy: f64, out: &mut Vec<SpatialNode<'a>>) {
        for c in children {
            match c {
                Child::Box(b) => match &b.kind {
                    BoxKind::Float { anchor, float, rect } => {
                        let n = self.stories.len();
                        self.story(StoryRef::Float { float: *float, anchor: *anchor }, b, dx, dy);
                        // A cell's float is placed in its unit: `rect` is relative to the parents' offsets.
                        self.stories[n].rect = [rect[0] + dx, rect[1] + dy, rect[2] + dx, rect[3] + dy];
                    }
                    BoxKind::Notes => {
                        for n in b.children.iter() {
                            if let Child::Box(nb) = n
                                && let BoxKind::Note { kind, id } = nb.kind
                            {
                                self.story(StoryRef::Note { kind, id }, nb, dx + b.offset.0, dy + b.offset.1);
                            }
                        }
                    }
                    BoxKind::Separator => {}
                    BoxKind::Body | BoxKind::Column | BoxKind::Note { .. } | BoxKind::Header { .. } | BoxKind::Footer { .. } => {
                        self.children(&b.children, dx + b.offset.0, dy + b.offset.1, out)
                    }
                },
                Child::Group { offset, children } => self.children(children, dx + offset.0, dy + offset.1, out),
                Child::Para(p) => p.lines.iter().for_each(|l| self.line(Some(p.para), l, dx, dy, out)),
                Child::Line { para, placed } => self.line(*para, placed, dx, dy, out),
                Child::Table(t) => {
                    let rows = t.rows.iter().map(|r| self.row(r, dx, dy)).collect();
                    out.push(SpatialNode::Table(TableNode { table: t.table, rows }));
                }
                Child::Items(_) => {}
            }
        }
    }

    fn row(&mut self, r: &'a RowFragment, dx: f64, dy: f64) -> RowNode<'a> {
        let shift = |q: [f64; 4]| [q[0] + dx, q[1] + dy, q[2] + dx, q[3] + dy];
        let cells = r
            .cells
            .iter()
            .map(|c| {
                let mut nodes = Vec::new();
                self.children(&c.content, dx, dy, &mut nodes);
                CellNode {
                    place: &c.place,
                    rect: shift(c.place.rect),
                    clip: c.place.clip.map(|y| y + dy),
                    closing: c.place.closing.map(|(p, [x, y, h])| (p, [x + dx, y + dy, h])),
                    nodes,
                }
            })
            .collect();
        RowNode {
            row: r.row,
            rect: shift(r.rect),
            top: r.top + dy,
            repeat: r.repeat,
            edges: r.edges.iter().map(|&(x, k)| (x + dx, k)).collect(),
            cells,
        }
    }
}

/// The characters a page's bodies show, by paragraph and unit: (paragraph, unit, character), in flow order. Artefacts
/// are left out: headers and footers, repeated header rows (TABLE-46), line numbers and text without a source (list
/// numbers, separators), and text pushed below the page (HDR-9).
pub fn semantic_chars(page: &PageFragment, out: &mut Vec<(ParaId, Unit, char)>) {
    fn run(it: &Item, dy: f64, h: f64, out: &mut Vec<(ParaId, Unit, char)>) {
        if let Item::Glyphs(r) = it
            && let Some(para) = r.para
            && !below_page(it, dy, h)
        {
            out.extend(r.chars.iter().filter_map(|&(c, _, u)| u.map(|u| (para, u, c))));
        }
    }
    fn walk(children: &[Child], dy: f64, h: f64, out: &mut Vec<(ParaId, Unit, char)>) {
        for c in children {
            match c {
                Child::Box(b) => walk(&b.children, dy + b.offset.1, h, out),
                Child::Group { offset, children } => walk(children, dy + offset.1, h, out),
                Child::Para(p) => {
                    for l in p.lines.iter() {
                        for it in l.line.items.iter() {
                            run(it, dy + l.y, h, out);
                        }
                    }
                }
                Child::Line { placed, .. } => {
                    for it in placed.line.items.iter() {
                        run(it, dy + placed.y, h, out);
                    }
                }
                Child::Table(t) => {
                    for r in t.rows.iter().filter(|r| !r.repeat) {
                        for cell in r.cells.iter() {
                            walk(&cell.content, dy, h, out);
                        }
                    }
                }
                Child::Items(items) => {
                    for it in items.iter() {
                        run(it, dy, h, out);
                    }
                }
            }
        }
    }
    // Headers and footers repeat on every page: artefacts.
    for c in page.children.iter() {
        let PageChild::Body(b) = c else { continue };
        walk(&b.children, b.offset.1, page.h, out);
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::super::fragment::*;
    use super::super::ids::{FloatId, FragmentId, ParaId, TableId};
    use super::*;

    fn rect(y: f64, colour: u32) -> Item {
        Item::Rect { rect: [0.0, y, 10.0, y + 1.0], colour }
    }

    fn line(colour: u32) -> PlacedLine {
        PlacedLine {
            x: 5.0,
            y: 0.0,
            line: Rc::new(LineFragment { items: Box::new([rect(0.0, colour)]), pitch: 12.0, baseline: 9.0, atoms: Box::new([]), changed: false }),
        }
    }

    fn page() -> PageFragment {
        let table = TableFragment {
            table: TableId(7),
            rows: Box::new([RowFragment {
                row: 0,
                rect: [0.0, 50.0, 100.0, 70.0],
                top: 50.0,
                edges: Box::new([(0.0, 0), (100.0, 1)]),
                background: Box::new([rect(50.0, 1)]),
                cells: Box::new([CellFragment {
                    cell: 0,
                    content: Box::new([Child::Line { para: Some(ParaId(3)), placed: line(2) }]),
                    place: CellPlace {
                        index: 0,
                        col: 0,
                        span: 1,
                        rect: [0.0, 50.0, 100.0, 70.0],
                        clip: None,
                        v_merge: crate::docx::VMerge::None,
                        hidden: Box::new([]),
                        closing: None,
                    },
                }]),
                borders: Box::new([rect(70.0, 3)]),
                repeat: false,
                changed: false,
            }]),
        };
        let column = BoxBuilder::new();
        let mut column = column;
        column.push(Child::Para(ParaFragment { para: ParaId(1), lines: Box::new([line(4)]), line_numbers: Box::new([]) }));
        column.push(Child::Table(table));
        let mut notes = BoxBuilder::new();
        notes.push(Child::Line { para: Some(ParaId(9)), placed: line(5) });
        let mut float = BoxBuilder::new();
        float.push(Child::Items(Box::new([rect(30.0, 6)])));
        let mut body = BoxBuilder::new();
        body.push(Child::Box(float.finish(
            BoxKind::Float { anchor: ParaId(1), float: FloatId(11), rect: [0.0, 0.0, 10.0, 10.0] },
            (0.0, 0.0),
            (10.0, 10.0),
        )));
        body.push(Child::Box(column.finish(BoxKind::Column, (72.0, 0.0), (468.0, 600.0))));
        body.push(Child::Box(notes.finish(BoxKind::Notes, (72.0, 700.0), (468.0, 20.0))));
        let mut header = BoxBuilder::new();
        header.push(Child::Line { para: Some(ParaId(10)), placed: line(7) });
        PageFragment {
            id: FragmentId::next(),
            w: 612.0,
            h: 792.0,
            body_area: Some((72.0, 720.0)),
            children: Box::new([
                PageChild::Body(Rc::new(body.finish(BoxKind::Body, (0.0, 0.0), (612.0, 792.0)))),
                PageChild::Header(header.finish(BoxKind::Header { story: "header1.xml".into() }, (0.0, 36.0), (612.0, 12.0))),
            ]),
        }
    }

    #[test]
    fn paint_order_is_background_flow_notes_floats_headers() {
        let colours: Vec<u32> = paint_items(&page()).iter().map(|it| if let Item::Rect { colour, .. } = it { *colour } else { 99 }).collect();
        // background (1), body flow: paragraph (4), cell content (2), row borders (3); notes (5); floats (6); header (7).
        assert_eq!(colours, vec![1, 4, 2, 3, 5, 6, 7]);
    }

    #[test]
    fn offsets_accumulate() {
        let items = paint_items(&page());
        // The paragraph line: column x 72 + line x 5.
        let Item::Rect { rect, .. } = &items[1] else { panic!() };
        assert_eq!(rect[0], 77.0);
        // The note line: notes box y 700.
        let Item::Rect { rect, .. } = &items[4] else { panic!() };
        assert_eq!(rect[1], 700.0);
    }

    #[test]
    fn spatial_lists_lines_and_rows_and_respects_provisional() {
        let p = page();
        assert!(matches!(spatial(&p, true), Spatial::Provisional));
        let Spatial::Page { lines, rows } = spatial(&p, false) else { panic!() };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].rect[0], 72.0);
        assert_eq!(lines.len(), 4);
        assert!(lines.windows(2).all(|w| w[0].rect[1] <= w[1].rect[1]));
    }

    #[test]
    fn text_below_the_page_is_excluded() {
        let face = crate::font::Fonts::system().face("Calibri", false, false);
        let Some(face) = face else { return };
        let run = |baseline: f64| {
            Item::Glyphs(GlyphRun {
                face: face.clone(),
                size: 11.0,
                baseline,
                x: 0.0,
                glyphs: Box::new([(1, 0.0)]),
                chars: Box::new([('a', 0.0, Some(Unit(0)))]),
                colour: None,
                para: Some(ParaId(1)),
            })
        };
        let line = PlacedLine {
            x: 0.0,
            y: 0.0,
            line: Rc::new(LineFragment {
                items: Box::new([run(100.0), run(900.0)]),
                pitch: 12.0,
                baseline: 9.0,
                atoms: Box::new([]),
                changed: false,
            }),
        };
        let mut body = BoxBuilder::new();
        body.push(Child::Para(ParaFragment { para: ParaId(1), lines: Box::new([line]), line_numbers: Box::new([]) }));
        let page = PageFragment {
            id: FragmentId::next(),
            w: 612.0,
            h: 792.0,
            body_area: None,
            children: Box::new([PageChild::Body(Rc::new(body.finish(BoxKind::Body, (0.0, 0.0), (612.0, 792.0))))]),
        };
        assert_eq!(paint_items(&page).len(), 1);
        assert_eq!(excluded(&page).len(), 1);
        let mut chars = Vec::new();
        semantic_chars(&page, &mut chars);
        assert_eq!(chars.len(), 1);
    }
}
