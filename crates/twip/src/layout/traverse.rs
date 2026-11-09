//! Fragment traversals

use crate::docx::NoteKind;

use super::fragment::{BoxFragment, BoxKind, CellPlace, Child, GlyphRun, Item, LineFragment, PageChild, PageFragment, PlacedLine, RowFragment};
use super::ids::{FloatId, ParaId, TableId, Unit};

#[derive(Clone, Copy)]
pub struct Placed<'a> {
    pub dx: f64,
    pub dy: f64,
    pub item: &'a Item,
    pub line: u32,
    pub own_colours: bool,
}

impl Placed<'_> {
    pub fn absolute(&self) -> Item {
        translated(self.item, self.dx, self.dy)
    }
}

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
        Item::Rect { rect, colour } => Item::rect(r4(rect), *colour),
        Item::Image { rect, key } => Item::Image { rect: r4(rect), key: *key },
        Item::Gridline { line } => Item::Gridline { line: r4(line) },
        Item::Shape(s) => Item::Shape(std::rc::Rc::new(s.moved(dx, dy))),
        Item::Turned(t) => Item::Turned(std::rc::Rc::new(super::fragment::Turned {
            centre: [t.centre[0] + dx, t.centre[1] + dy],
            deg: t.deg,
            items: t.items.iter().map(|i| translated(i, dx, dy)).collect(),
        })),
    }
}

fn below_page(item: &Item, dy: f64, h: f64) -> bool {
    matches!(item, Item::Glyphs(r) if r.baseline + dy - r.size >= h)
}

// Paint order
struct Lines<'a, 'f> {
    n: u32,
    placed: &'f mut dyn FnMut(u32, Option<ParaId>, &'a PlacedLine, f64, f64),
}

impl Lines<'_, '_> {
    fn next(&mut self) -> u32 {
        self.n += 1;
        self.n
    }
}

fn line_items<'a>(para: Option<ParaId>, p: &'a PlacedLine, dx: f64, dy: f64, lines: &mut Lines<'a, '_>, out: &mut dyn FnMut(Placed<'a>)) {
    let line = lines.next();
    (lines.placed)(line, para, p, dx, dy);
    for it in p.line.items.iter() {
        out(Placed { dx: dx + p.x, dy: dy + p.y, item: it, line, own_colours: false });
    }
}

fn backgrounds<'a>(children: &'a [Child], dx: f64, dy: f64, skip: &dyn Fn(&BoxKind) -> bool, out: &mut dyn FnMut(Placed<'a>)) {
    for c in children {
        match c {
            Child::Box(b) if skip(&b.kind) => {}
            Child::Box(b) => backgrounds(&b.children, dx + b.offset.0, dy + b.offset.1, skip, out),
            Child::Group { offset, children } => backgrounds(children, dx + offset.0, dy + offset.1, skip, out),
            Child::Table(t) => {
                for r in t.rows.iter() {
                    for it in r.background.iter() {
                        out(Placed { dx, dy, item: it, line: 0, own_colours: false });
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

fn flow<'a>(children: &'a [Child], dx: f64, dy: f64, skip: &dyn Fn(&BoxKind) -> bool, lines: &mut Lines<'a, '_>, out: &mut dyn FnMut(Placed<'a>)) {
    for c in children {
        match c {
            Child::Box(b) if skip(&b.kind) => {}
            Child::Box(b) => flow(&b.children, dx + b.offset.0, dy + b.offset.1, skip, lines, out),
            Child::Group { offset, children } => flow(children, dx + offset.0, dy + offset.1, skip, lines, out),
            Child::Para(p) => {
                for l in p.lines.iter() {
                    line_items(Some(p.para), l, dx, dy, lines, out);
                }
                for label in p.line_numbers.iter() {
                    let line = lines.next();
                    for it in label.iter() {
                        out(Placed { dx, dy, item: it, line, own_colours: false });
                    }
                }
            }
            Child::Table(t) => {
                for r in t.rows.iter() {
                    row(r, dx, dy, skip, lines, out);
                }
            }
            Child::Line { para, placed } => line_items(*para, placed, dx, dy, lines, out),
            Child::Items(items) => {
                let line = lines.next();
                for it in items.iter() {
                    out(Placed { dx, dy, item: it, line, own_colours: false });
                }
            }
        }
    }
}

fn row<'a>(r: &'a RowFragment, dx: f64, dy: f64, skip: &dyn Fn(&BoxKind) -> bool, lines: &mut Lines<'a, '_>, out: &mut dyn FnMut(Placed<'a>)) {
    for cell in r.cells.iter() {
        flow(&cell.content, dx, dy, skip, lines, out);
    }
    for it in r.borders.iter() {
        out(Placed { dx, dy, item: it, line: 0, own_colours: false });
    }
}

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

fn filled(children: &[Child]) -> bool {
    fn shapes(items: &[Item]) -> bool {
        items.iter().any(|i| match i {
            Item::Shape(s) => !s.fill.is_none() && !s.picture,
            Item::Turned(t) => shapes(&t.items),
            _ => false,
        })
    }
    children.iter().any(|c| match c {
        Child::Items(i) => shapes(i),
        Child::Group { children, .. } => filled(children),
        Child::Box(b) => filled(&b.children),
        _ => false,
    })
}

fn is_float(k: &BoxKind) -> bool {
    matches!(k, BoxKind::Float { .. })
}

fn is_notes(k: &BoxKind) -> bool {
    matches!(k, BoxKind::Notes)
}

pub fn paint<'a>(page: &'a PageFragment, out: &mut dyn FnMut(Placed<'a>)) {
    paint_lines(page, out, &mut |_, _, _, _, _| {});
}

pub fn paint_lines<'a>(
    page: &'a PageFragment,
    out: &mut dyn FnMut(Placed<'a>),
    placed: &mut dyn FnMut(u32, Option<ParaId>, &'a PlacedLine, f64, f64),
) {
    let h = page.h;
    let mut visible = |p: Placed<'a>| {
        if !below_page(p.item, p.dy, h) {
            out(p);
        }
    };
    let mut lines = Lines { n: 0, placed };
    for item in page.borders.iter() {
        visible(Placed { dx: 0.0, dy: 0.0, item, line: 0, own_colours: false });
    }
    let bodies = || page.children.iter().filter_map(|c| if let PageChild::Body(b) = c { Some(&**b) } else { None });
    let neither = |k: &BoxKind| is_float(k) || is_notes(k);
    for b in bodies() {
        backgrounds(&b.children, b.offset.0, b.offset.1, &neither, &mut visible);
    }
    let stories = || {
        page.children.iter().map(|c| match c {
            PageChild::Body(b) => &**b,
            PageChild::Header(b) | PageChild::Footer(b) => b,
        })
    };
    let behind = |k: &BoxKind| matches!(k, BoxKind::Float { layer, .. } if layer.behind);
    let mut found = Vec::new();
    for b in stories() {
        boxes_of(&b.children, b.offset.0, b.offset.1, &behind, &mut found);
    }
    found.sort_by_key(|(fb, ..)| layer_of(&fb.kind));
    for (fb, dx, dy) in found {
        let own_colours = filled(&fb.children);
        let mut inside = |p: Placed<'a>| visible(Placed { own_colours, ..p });
        backgrounds(&fb.children, dx + fb.offset.0, dy + fb.offset.1, &|_| false, &mut inside);
        flow(&fb.children, dx + fb.offset.0, dy + fb.offset.1, &|_| false, &mut lines, &mut inside);
    }
    for b in bodies() {
        flow(&b.children, b.offset.0, b.offset.1, &neither, &mut lines, &mut visible);
    }
    let in_front = |k: &BoxKind| is_float(k) && !behind(k);
    for want in [&is_notes as &dyn Fn(&BoxKind) -> bool, &in_front] {
        let mut found = Vec::new();
        for b in bodies() {
            boxes_of(&b.children, b.offset.0, b.offset.1, want, &mut found);
        }
        found.sort_by_key(|(fb, ..)| layer_of(&fb.kind));
        for (fb, dx, dy) in found {
            let own_colours = filled(&fb.children);
            let mut inside = |p: Placed<'a>| visible(Placed { own_colours, ..p });
            backgrounds(&fb.children, dx + fb.offset.0, dy + fb.offset.1, &|_| false, &mut inside);
            flow(&fb.children, dx + fb.offset.0, dy + fb.offset.1, &|_| false, &mut lines, &mut inside);
        }
    }
    for c in page.children.iter() {
        if let PageChild::Header(b) | PageChild::Footer(b) = c {
            backgrounds(&b.children, b.offset.0, b.offset.1, &behind, &mut visible);
            flow(&b.children, b.offset.0, b.offset.1, &behind, &mut lines, &mut visible);
        }
    }
}

pub fn lines_in<'a>(children: &'a [Child], placed: &mut dyn FnMut(Option<ParaId>, &'a PlacedLine, f64, f64)) {
    let mut each = |_: u32, para: Option<ParaId>, l: &'a PlacedLine, dx: f64, dy: f64| placed(para, l, dx, dy);
    let mut lines = Lines { n: 0, placed: &mut each };
    flow(children, 0.0, 0.0, &is_float, &mut lines, &mut |_| {});
}

pub fn items_of(children: &[Child]) -> Vec<Item> {
    let mut v = Vec::new();
    let mut out = |p: Placed| v.push(p.absolute());
    backgrounds(children, 0.0, 0.0, &|_| false, &mut out);
    flow(children, 0.0, 0.0, &|_| false, &mut Lines { n: 0, placed: &mut |_, _, _, _, _| {} }, &mut out);
    v
}

fn layer_of(k: &BoxKind) -> Option<super::fragment::Layer> {
    match k {
        BoxKind::Float { layer, .. } => Some(*layer),
        _ => None,
    }
}
pub fn paint_float(page: &PageFragment, id: FloatId) -> Vec<Item> {
    let mut found = Vec::new();
    for c in page.children.iter() {
        let b: &BoxFragment = match c {
            PageChild::Body(b) => b,
            PageChild::Header(b) | PageChild::Footer(b) => b,
        };
        boxes_of(&b.children, b.offset.0, b.offset.1, &|k| matches!(k, BoxKind::Float { float, .. } if *float == id), &mut found);
    }
    let mut v = Vec::new();
    let mut out = |p: Placed| v.push(p.absolute());
    for (fb, dx, dy) in found {
        let (dx, dy) = (dx + fb.offset.0, dy + fb.offset.1);
        backgrounds(&fb.children, dx, dy, &|_| false, &mut out);
        flow(&fb.children, dx, dy, &|_| false, &mut Lines { n: 0, placed: &mut |_, _, _, _, _| {} }, &mut out);
    }
    v
}

pub fn paint_items(page: &PageFragment) -> Vec<Item> {
    let mut v = Vec::new();
    paint(page, &mut |p| v.push(p.absolute()));
    v
}

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
    flow(&body.children, body.offset.0, body.offset.1, &|_| false, &mut Lines { n: 0, placed: &mut |_, _, _, _, _| {} }, &mut each);
    out
}

pub fn excluded(page: &PageFragment) -> Vec<(Option<ParaId>, Vec<Unit>)> {
    let mut all = Vec::new();
    let mut each = |p: Placed| {
        if let Item::Glyphs(r) = p.item
            && below_page(p.item, p.dy, page.h)
        {
            all.push((r.para, r.chars.iter().filter_map(|c| c.2).collect()));
        }
    };
    for c in page.children.iter() {
        match c {
            PageChild::Body(b) => {
                flow(&b.children, b.offset.0, b.offset.1, &|_| false, &mut Lines { n: 0, placed: &mut |_, _, _, _, _| {} }, &mut each)
            }
            PageChild::Header(b) | PageChild::Footer(b) => {
                flow(&b.children, b.offset.0, b.offset.1, &|_| false, &mut Lines { n: 0, placed: &mut |_, _, _, _, _| {} }, &mut each)
            }
        }
    }
    all
}

// Spatial order
#[derive(Clone, Debug)]
pub struct SpatialLine {
    pub rect: [f64; 4],
    pub para: Option<ParaId>,
    pub units: Option<(Unit, Unit)>,
}

#[derive(Clone, Debug)]
pub struct SpatialRow {
    pub table: TableId,
    pub row: usize,
    pub rect: [f64; 4],
    pub top: f64,
    pub edges: Vec<(f64, usize)>,
}

pub enum Spatial {
    Provisional,
    Page { lines: Vec<SpatialLine>, rows: Vec<SpatialRow> },
}

pub fn spatial(page: &PageFragment, provisional: bool) -> Spatial {
    if provisional {
        return Spatial::Provisional;
    }
    let (mut lines, mut rows) = (Vec::new(), Vec::new());
    fn add(lines: &mut Vec<SpatialLine>, para: Option<ParaId>, p: &PlacedLine, dx: f64, dy: f64, h: f64) {
        {
            let (x, y) = (dx + p.x, dy + p.y);
            let mut runs = p.line.items.iter().filter(|it| matches!(it, Item::Glyphs(_))).peekable();
            if runs.peek().is_some() && runs.all(|it| below_page(it, y, h)) {
                return;
            }
            let first = lines.len();
            for it in p.line.items.iter() {
                let Item::Glyphs(r) = it else { continue };
                let q = r.para.or(para);
                let k = match lines[first..].iter().position(|l| l.para == q) {
                    Some(k) => first + k,
                    None => {
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

// Spatial tree
pub struct SpatialTree<'a> {
    pub stories: Vec<SpatialStory<'a>>,
}

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

pub struct LineNode<'a> {
    pub para: Option<ParaId>,
    pub x: f64,
    pub y: f64,
    pub line: &'a std::rc::Rc<LineFragment>,
}

pub struct TableNode<'a> {
    pub table: TableId,
    pub rows: Vec<RowNode<'a>>,
}

pub struct RowNode<'a> {
    pub row: usize,
    pub rect: [f64; 4],
    pub top: f64,
    pub repeat: bool,
    pub edges: Vec<(f64, usize)>,
    pub cells: Vec<CellNode<'a>>,
}

pub struct CellNode<'a> {
    pub place: &'a CellPlace,
    pub rect: [f64; 4],
    pub clip: Option<f64>,
    pub closing: Option<(ParaId, [f64; 3])>,
    pub nodes: Vec<SpatialNode<'a>>,
}

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
        let mut runs = p.line.items.iter().filter(|it| matches!(it, Item::Glyphs(_))).peekable();
        if runs.peek().is_some() && runs.all(|it| below_page(it, y, self.h)) {
            return;
        }
        out.push(SpatialNode::Line(LineNode { para, x, y, line: &p.line }));
    }

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
        let rect = if r[0].is_finite() { [r[0], r[1], r[2].max(r[0] + b.size.0), r[3]] } else { [x, y, x + b.size.0, y + b.size.1] };
        self.stories.push(SpatialStory { story, rect, nodes });
    }

    fn children(&mut self, children: &'a [Child], dx: f64, dy: f64, out: &mut Vec<SpatialNode<'a>>) {
        for c in children {
            match c {
                Child::Box(b) => match &b.kind {
                    BoxKind::Float { anchor, float, rect, .. } => {
                        let n = self.stories.len();
                        self.story(StoryRef::Float { float: *float, anchor: *anchor }, b, dx, dy);
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

// Semantic order
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
        Item::rect([0.0, y, 10.0, y + 1.0], colour)
    }

    fn line(colour: u32) -> PlacedLine {
        PlacedLine {
            x: 5.0,
            y: 0.0,
            line: Rc::new(LineFragment {
                items: Box::new([rect(0.0, colour)]),
                pitch: 12.0,
                baseline: 9.0,
                atoms: Box::new([]),
                changed: false,
                mark: None,
                texts: Box::new([]),
                expansions: Box::new([]),
            }),
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
            BoxKind::Float { anchor: ParaId(1), float: FloatId(11), rect: [0.0, 0.0, 10.0, 10.0], layer: Default::default() },
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
            borders: Box::new([]),
            children: Box::new([
                PageChild::Body(Rc::new(body.finish(BoxKind::Body, (0.0, 0.0), (612.0, 792.0)))),
                PageChild::Header(header.finish(BoxKind::Header { story: "header1.xml".into() }, (0.0, 36.0), (612.0, 12.0))),
            ]),
        }
    }

    #[test]
    fn paint_order_is_background_flow_notes_floats_headers() {
        let colours: Vec<u32> = paint_items(&page()).iter().map(|it| if let Item::Rect { colour, .. } = it { *colour } else { 99 }).collect();
        assert_eq!(colours, vec![1, 4, 2, 3, 5, 6, 7]);
    }

    #[test]
    fn offsets_accumulate() {
        let items = paint_items(&page());
        let Item::Rect { rect, .. } = &items[1] else { panic!() };
        assert_eq!(rect[0], 77.0);
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
        let face = crate::font::Fonts::system_recommended().face("Calibri", None, false, false);
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
                mark: None,
                texts: Box::new([]),
                expansions: Box::new([]),
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
            borders: Box::new([]),
        };
        assert_eq!(paint_items(&page).len(), 1);
        assert_eq!(excluded(&page).len(), 1);
        let mut chars = Vec::new();
        semantic_chars(&page, &mut chars);
        assert_eq!(chars.len(), 1);
    }
}
