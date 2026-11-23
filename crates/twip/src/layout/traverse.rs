//! Fragment traversals

use crate::docx::NoteKind;

use super::fragment::Clipped;
use super::fragment::{
    BoxFragment, BoxKind, CellFragment, CellPlace, Child, Item, LineFragment, PageChild, PageFragment, PlacedLine, RowFragment, translated,
};
use super::ids::{FloatId, ParaId, TableId, Unit};
use super::vertical::{Turn, Turning};

#[derive(Clone, Copy)]
pub struct Placed<'a> {
    pub dx: f64,
    pub dy: f64,
    pub item: &'a Item,
    pub line: u32,
    pub own_colours: bool,
    /// The float whose box it is drawn in.
    pub float: Option<FloatId>,
    /// The box on the page it is clipped to: an exact-height row's cell.
    pub clip: Option<[f64; 4]>,
}

impl Placed<'_> {
    pub fn absolute(&self) -> Item {
        translated(self.item, self.dx, self.dy)
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
        out(Placed { dx: dx + p.x, dy: dy + p.y, item: it, line, own_colours: false, float: None, clip: None });
    }
}

fn backgrounds<'a>(children: &'a [Child], dx: f64, dy: f64, skip: &dyn Fn(&BoxKind) -> bool, out: &mut dyn FnMut(Placed<'a>)) {
    for c in children {
        match c {
            Child::Box(b) if skip(&b.kind) => {}
            Child::Box(b) => match float_of(&b.kind) {
                Some(f) => backgrounds(&b.children, dx + b.offset.0, dy + b.offset.1, skip, &mut |p| out(Placed { float: p.float.or(Some(f)), ..p })),
                None => backgrounds(&b.children, dx + b.offset.0, dy + b.offset.1, skip, out),
            },
            Child::Group { offset, children } => backgrounds(children, dx + offset.0, dy + offset.1, skip, out),
            Child::Table(t) => {
                for r in t.rows.iter() {
                    for it in r.background.iter() {
                        out(Placed { dx, dy, item: it, line: 0, own_colours: false, float: None, clip: None });
                    }
                    for cell in r.cells.iter() {
                        clipped(cell, dx, dy, out, &mut |out| backgrounds(&cell.content, dx, dy, skip, out));
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
            Child::Box(b) => match float_of(&b.kind) {
                Some(f) => flow(&b.children, dx + b.offset.0, dy + b.offset.1, skip, lines, &mut |p| out(Placed { float: p.float.or(Some(f)), ..p })),
                None => flow(&b.children, dx + b.offset.0, dy + b.offset.1, skip, lines, out),
            },
            Child::Group { offset, children } => flow(children, dx + offset.0, dy + offset.1, skip, lines, out),
            Child::Para(p) => {
                for l in p.lines.iter() {
                    line_items(Some(p.para), l, dx, dy, lines, out);
                }
                for label in p.line_numbers.iter() {
                    let line = lines.next();
                    for it in label.iter() {
                        out(Placed { dx, dy, item: it, line, own_colours: false, float: None, clip: None });
                    }
                }
            }
            Child::Table(t) => {
                for r in t.rows.iter() {
                    row(r, dx, dy, skip, lines, out);
                }
            }
            Child::Line { para, placed } => line_items(*para, placed, dx, dy, lines, out),
            Child::Items(items) | Child::Turned { items, .. } => {
                let line = lines.next();
                for it in items.iter() {
                    out(Placed { dx, dy, item: it, line, own_colours: false, float: None, clip: None });
                }
            }
        }
    }
}

fn row<'a>(r: &'a RowFragment, dx: f64, dy: f64, skip: &dyn Fn(&BoxKind) -> bool, lines: &mut Lines<'a, '_>, out: &mut dyn FnMut(Placed<'a>)) {
    for cell in r.cells.iter() {
        clipped(cell, dx, dy, out, &mut |out| flow(&cell.content, dx, dy, skip, lines, out));
    }
    for it in r.borders.iter() {
        out(Placed { dx, dy, item: it, line: 0, own_colours: false, float: None, clip: None });
    }
}

/// What `draw` gives `out` from `cell`, clipped to the cell above its clip when its row's height is exact.
fn clipped<'a>(cell: &CellFragment, dx: f64, dy: f64, out: &mut dyn FnMut(Placed<'a>), draw: &mut dyn FnMut(&mut dyn FnMut(Placed<'a>))) {
    let Some(b) = cell_clip(cell, dx, dy) else { return draw(out) };
    draw(&mut |p| out(Placed { clip: meet(p.clip, Some(b)), ..p }));
}

/// The box on the page `cell` is clipped to when its row's height is exact.
fn cell_clip(cell: &CellFragment, dx: f64, dy: f64) -> Option<[f64; 4]> {
    let bottom = cell.place.clip?;
    let [x0, y0, x1, _] = cell.place.rect;
    Some([x0 + dx, y0 + dy, x1 + dx, bottom + dy])
}

/// The clip that is both `a` and `b`, either of which may be open.
fn meet(a: Option<[f64; 4]>, b: Option<[f64; 4]>) -> Option<[f64; 4]> {
    match (a, b) {
        (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
        (a, b) => a.or(b),
    }
}

/// A box found by [`boxes_of`], with its offset and the clip it is drawn under.
type Found<'a> = (&'a BoxFragment, f64, f64, Option<[f64; 4]>);

fn boxes_of<'a>(children: &'a [Child], dx: f64, dy: f64, clip: Option<[f64; 4]>, want: &dyn Fn(&BoxKind) -> bool, out: &mut Vec<Found<'a>>) {
    for c in children {
        match c {
            Child::Box(b) if want(&b.kind) => out.push((b, dx, dy, clip)),
            Child::Box(b) => boxes_of(&b.children, dx + b.offset.0, dy + b.offset.1, clip, want, out),
            Child::Group { offset, children } => boxes_of(children, dx + offset.0, dy + offset.1, clip, want, out),
            Child::Table(t) => {
                for cell in t.rows.iter().flat_map(|r| r.cells.iter()) {
                    boxes_of(&cell.content, dx, dy, meet(clip, cell_clip(cell, dx, dy)), want, out);
                }
            }
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
        Child::Items(i) | Child::Turned { items: i, .. } => shapes(i),
        Child::Group { children, .. } => filled(children),
        Child::Box(b) => filled(&b.children),
        _ => false,
    })
}

fn is_float(k: &BoxKind) -> bool {
    matches!(k, BoxKind::Float { .. })
}

fn float_of(k: &BoxKind) -> Option<FloatId> {
    match k {
        BoxKind::Float { float, .. } => Some(*float),
        _ => None,
    }
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
    let borders = |visible: &mut dyn FnMut(Placed<'a>)| {
        for item in page.borders.iter() {
            visible(Placed { dx: 0.0, dy: 0.0, item, line: 0, own_colours: false, float: None, clip: None });
        }
    };
    if page.borders_behind {
        borders(&mut visible);
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
        boxes_of(&b.children, b.offset.0, b.offset.1, None, &behind, &mut found);
    }
    found.sort_by_key(|(fb, ..)| layer_of(&fb.kind));
    for (fb, dx, dy, clip) in found {
        let own_colours = filled(&fb.children);
        let float = float_of(&fb.kind);
        let mut inside = |p: Placed<'a>| visible(Placed { own_colours, float: p.float.or(float), clip: meet(clip, p.clip), ..p });
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
            boxes_of(&b.children, b.offset.0, b.offset.1, None, want, &mut found);
        }
        found.sort_by_key(|(fb, ..)| layer_of(&fb.kind));
        for (fb, dx, dy, clip) in found {
            let own_colours = filled(&fb.children);
            let float = float_of(&fb.kind);
            let mut inside = |p: Placed<'a>| visible(Placed { own_colours, float: p.float.or(float), clip: meet(clip, p.clip), ..p });
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
    if !page.borders_behind {
        borders(&mut visible);
    }
}

pub fn lines_in<'a>(children: &'a [Child], placed: &mut dyn FnMut(Option<ParaId>, &'a PlacedLine, f64, f64)) {
    let mut each = |_: u32, para: Option<ParaId>, l: &'a PlacedLine, dx: f64, dy: f64| placed(para, l, dx, dy);
    let mut lines = Lines { n: 0, placed: &mut each };
    flow(children, 0.0, 0.0, &is_float, &mut lines, &mut |_| {});
}

pub fn items_of(children: &[Child]) -> Vec<Item> {
    let mut v = Vec::new();
    each_item(children, &mut |p| v.push(p.absolute()));
    v
}

/// [`items_of`] with the box each item is clipped to.
pub fn clipped_items_of(children: &[Child]) -> Vec<Clipped> {
    let mut v = Vec::new();
    each_item(children, &mut |p| v.push((p.absolute(), p.clip)));
    v
}

fn each_item<'a>(children: &'a [Child], out: &mut dyn FnMut(Placed<'a>)) {
    backgrounds(children, 0.0, 0.0, &|_| false, out);
    flow(children, 0.0, 0.0, &|_| false, &mut Lines { n: 0, placed: &mut |_, _, _, _, _| {} }, out);
}

fn layer_of(k: &BoxKind) -> Option<super::fragment::Layer> {
    match k {
        BoxKind::Float { layer, .. } => Some(*layer),
        _ => None,
    }
}
pub fn paint_float(page: &PageFragment, id: FloatId) -> Vec<Item> {
    paint_float_clipped(page, id).into_iter().map(|(item, _)| item).collect()
}

/// [`paint_float`] with the box each item is clipped to.
pub fn paint_float_clipped(page: &PageFragment, id: FloatId) -> Vec<Clipped> {
    let mut found = Vec::new();
    for c in page.children.iter() {
        let b: &BoxFragment = match c {
            PageChild::Body(b) => b,
            PageChild::Header(b) | PageChild::Footer(b) => b,
        };
        boxes_of(&b.children, b.offset.0, b.offset.1, None, &|k| matches!(k, BoxKind::Float { float, .. } if *float == id), &mut found);
    }
    let mut v = Vec::new();
    for (fb, dx, dy, clip) in found {
        let mut out = |p: Placed| v.push((p.absolute(), meet(clip, p.clip)));
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
                Child::Items(_) | Child::Turned { .. } => {}
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
    /// A float's rotation on the page, which turns `rect` about the float's centre; none for other stories.
    pub turn: Turn,
    /// Where the frame a float or note is laid out in is drawn on the page; none for other stories.
    pub region: Turn,
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
    /// The line's items as laid out: a vertical section's line's without the turn they are drawn with.
    pub items: &'a [Item],
    /// Where the line, laid out at `x`, `y`, is drawn on the page.
    pub turn: Turn,
    /// Where the boxes laid out with it are drawn: `turn`, unless lines are turned each about their own band.
    pub region: Turn,
    /// The page directions its text runs and its lines stack by text flow alone, as [`super::vertical::Unturned::axes`].
    pub axes: [f64; 4],
}

pub struct TableNode<'a> {
    pub table: TableId,
    pub rows: Vec<RowNode<'a>>,
}

pub struct RowNode<'a> {
    /// Where its rects and edges, as laid out, are drawn on the page.
    pub turn: Turn,
    pub row: usize,
    pub rect: [f64; 4],
    pub top: f64,
    pub repeat: bool,
    pub edges: Vec<(f64, usize)>,
    pub cells: Vec<CellNode<'a>>,
}

pub struct CellNode<'a> {
    /// Where its rect, clip and closing caret, as laid out, are drawn on the page.
    pub turn: Turn,
    pub place: &'a CellPlace,
    pub rect: [f64; 4],
    pub clip: Option<f64>,
    pub closing: Option<(ParaId, [f64; 3])>,
    pub nodes: Vec<SpatialNode<'a>>,
}

/// Each line among `nodes`, those in table cells included.
pub fn each_line<'a>(nodes: &[SpatialNode<'a>], f: &mut dyn FnMut(&LineNode<'a>)) {
    for n in nodes {
        match n {
            SpatialNode::Line(l) => f(l),
            SpatialNode::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| each_line(&c.nodes, f)),
        }
    }
}

pub fn spatial_tree(page: &PageFragment, provisional: bool) -> Option<SpatialTree<'_>> {
    if provisional {
        return None;
    }
    let mut w = SpatialWalk { h: page.h, stories: Vec::new(), frame: Frame::PAGE };
    let mut bodies = Vec::new();
    for c in page.children.iter() {
        if let PageChild::Body(b) = c {
            let mut nodes = Vec::new();
            w.children(&b.children, b.offset.0, b.offset.1, &mut nodes);
            let (x, y) = b.offset;
            bodies.push(SpatialStory {
                story: StoryRef::Body,
                rect: [x, y, x + b.size.0, y + b.size.1],
                turn: Turn::NONE,
                region: Turn::NONE,
                nodes,
            });
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
    frame: Frame,
}

/// Where the content being walked is drawn: `post` places what `turning` (a vertical section's body) turns, or the
/// content itself without one.
#[derive(Clone, Copy)]
struct Frame {
    post: Turn,
    turning: Option<Turning>,
    axes: [f64; 4],
    /// Lines hold their items turned, in one item.
    peel: bool,
}

impl Frame {
    const PAGE: Frame = Frame { post: Turn::NONE, turning: None, axes: Turn::NONE.m, peel: false };

    fn rects(&self) -> Turn {
        self.turning.map_or(self.post, |t| t.turn.then(self.post))
    }

    fn line(&self, top: f64, height: f64) -> Turn {
        self.turning.map_or(self.post, |t| t.line(top, height).then(self.post))
    }
}

/// The rotation of the float whose children are `children` and whose frame is `rect`: its own shape's, about the
/// frame's centre.
fn float_turn(children: &[Child], rect: [f64; 4]) -> Turn {
    let shape = children.iter().find_map(|c| match c {
        Child::Items(items) => items.iter().find_map(|i| match i {
            Item::Shape(s) if s.path.is_empty() && !s.decoration && s.rot.rem_euclid(360.0) > 1e-9 => Some(s.rot),
            _ => None,
        }),
        _ => None,
    });
    shape.map_or(Turn::NONE, |deg| Turn::about([(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0], deg))
}

/// `outer`'s linear part after `inner`'s.
fn axes_then(inner: [f64; 4], outer: [f64; 4]) -> [f64; 4] {
    Turn { m: inner, t: [0.0; 2] }.then(Turn { m: outer, t: [0.0; 2] }).m
}

impl<'a> SpatialWalk<'a> {
    fn line(&self, para: Option<ParaId>, p: &'a PlacedLine, dx: f64, dy: f64, out: &mut Vec<SpatialNode<'a>>) {
        let (x, y) = (dx + p.x, dy + p.y);
        let f = &self.frame;
        let items: &[Item] = match &*p.line.items {
            [Item::Turned(t)] if f.peel => &t.items,
            items => items,
        };
        if f.turning.is_none() && f.post == Turn::NONE {
            let mut runs = items.iter().filter(|it| matches!(it, Item::Glyphs(_))).peekable();
            if runs.peek().is_some() && runs.all(|it| below_page(it, y, self.h)) {
                return;
            }
        }
        let turn = f.line(y, p.line.pitch - p.line.below);
        out.push(SpatialNode::Line(LineNode { para, x, y, line: &p.line, items, turn, region: f.rects(), axes: f.axes }));
    }

    /// `children` walked in `frame`.
    fn within(&mut self, frame: Frame, children: &'a [Child], out: &mut Vec<SpatialNode<'a>>) {
        let outer = std::mem::replace(&mut self.frame, frame);
        self.children(children, 0.0, 0.0, out);
        self.frame = outer;
    }

    /// The lines of turned text drawn by `items` at `dx`, `dy`.
    fn unturned(&mut self, items: &[Item], text: &'a [super::vertical::Unturned], dx: f64, dy: f64, out: &mut Vec<SpatialNode<'a>>) {
        let f = self.frame;
        for (k, u) in text.iter().enumerate() {
            // Lines turned each about their own band
            let outer = match f.turning {
                Some(t) if t.each_line => {
                    let c = match items.get(k).or(items.first()) {
                        Some(Item::Turned(i)) => i.centre[1],
                        Some(i) => i.top(),
                        None => 0.0,
                    };
                    t.line(dy + c, 0.0).then(f.post)
                }
                _ => f.rects(),
            };
            let post = u.turn.then(Turn::NONE.shifted(dx, dy)).then(outer);
            self.within(Frame { post, turning: None, axes: axes_then(u.axes, f.axes), peel: false }, &u.children, out);
        }
    }

    fn story(&mut self, story: StoryRef<'a>, b: &'a BoxFragment, dx: f64, dy: f64) {
        let (x, y) = (dx + b.offset.0, dy + b.offset.1);
        let mut nodes = Vec::new();
        self.children(&b.children, x, y, &mut nodes);
        // Extent as laid out, in the frame the box is laid out in
        let f = self.frame;
        let region = f.rects();
        let back = region.inverse();
        let mut r = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        let mut add = |q: [f64; 4]| r = [r[0].min(q[0]), r[1].min(q[1]), r[2].max(q[2]), r[3].max(q[3])];
        for n in &nodes {
            match n {
                SpatialNode::Line(l) => {
                    let q = [l.x, l.y, l.x + 1.0, l.y + l.line.pitch];
                    add(if l.turn == f.line(l.y, l.line.pitch - l.line.below) { q } else { back.rect(l.turn.rect(q)) })
                }
                SpatialNode::Table(t) => {
                    t.rows.iter().for_each(|row| add(if row.turn == region { row.rect } else { back.rect(row.turn.rect(row.rect)) }))
                }
            }
        }
        let laid = if r[0].is_finite() { [r[0], r[1], r[2].max(r[0] + b.size.0), r[3]] } else { [x, y, x + b.size.0, y + b.size.1] };
        self.stories.push(SpatialStory { story, rect: region.rect(laid), turn: Turn::NONE, region, nodes });
    }

    fn children(&mut self, children: &'a [Child], dx: f64, dy: f64, out: &mut Vec<SpatialNode<'a>>) {
        for c in children {
            match c {
                Child::Box(b) => match &b.kind {
                    BoxKind::Float { anchor, float, rect, .. } => {
                        let n = self.stories.len();
                        self.story(StoryRef::Float { float: *float, anchor: *anchor }, b, dx, dy);
                        let r = self.frame.rects().rect([rect[0] + dx, rect[1] + dy, rect[2] + dx, rect[3] + dy]);
                        self.stories[n].rect = r;
                        self.stories[n].turn = float_turn(&b.children, r);
                        self.stories[n].region = self.frame.rects();
                    }
                    BoxKind::Turned(turning) => {
                        let f = self.frame;
                        let post = Turn::NONE.shifted(dx + b.offset.0, dy + b.offset.1).then(f.rects());
                        let axes = axes_then(turning.turn.m, f.axes);
                        self.within(Frame { post, turning: Some(*turning), axes, peel: true }, &b.children, out);
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
                Child::Turned { items, text } => self.unturned(items, text, dx, dy, out),
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
                    turn: self.frame.rects(),
                    place: &c.place,
                    rect: shift(c.place.rect),
                    clip: c.place.clip.map(|y| y + dy),
                    closing: c.place.closing.map(|(p, [x, y, h])| (p, [x + dx, y + dy, h])),
                    nodes,
                }
            })
            .collect();
        RowNode {
            turn: self.frame.rects(),
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
                Child::Items(items) | Child::Turned { items, .. } => {
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
                below: 0.0,
                after: 0.0,
                atoms: Box::new([]),
                changed: false,
                mark: None,
                texts: Box::new([]),
                expansions: Box::new([]),
                label: None,
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
            borders_behind: false,
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
                paint: None,
            })
        };
        let line = PlacedLine {
            x: 0.0,
            y: 0.0,
            line: Rc::new(LineFragment {
                items: Box::new([run(100.0), run(900.0)]),
                pitch: 12.0,
                baseline: 9.0,
                below: 0.0,
                after: 0.0,
                atoms: Box::new([]),
                changed: false,
                mark: None,
                texts: Box::new([]),
                expansions: Box::new([]),
                label: None,
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
            borders_behind: false,
        };
        assert_eq!(paint_items(&page).len(), 1);
        assert_eq!(excluded(&page).len(), 1);
        let mut chars = Vec::new();
        semantic_chars(&page, &mut chars);
        assert_eq!(chars.len(), 1);
    }
}
