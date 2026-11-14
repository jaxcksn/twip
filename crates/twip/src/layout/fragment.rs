//! Layout fragments

use std::rc::Rc;

use crate::docx::NoteKind;
use crate::font::Face;

use crate::docx::VMerge;

use super::ids::{FloatId, FragmentId, ParaId, TableId, Unit};

#[derive(Clone, Debug)]
pub struct GlyphRun {
    pub face: Rc<Face>,
    pub size: f64,
    pub baseline: f64,
    pub x: f64,
    pub glyphs: Box<[(u32, f64)]>,
    pub chars: Box<[(char, f64, Option<Unit>)]>,
    pub colour: Option<u32>,
    pub para: Option<ParaId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey {
    pub index: Option<usize>,
    pub generation: u32,
}

#[derive(Clone, Debug)]
pub enum Item {
    Glyphs(GlyphRun),
    Rect { rect: [f64; 4], colour: u32 },
    Image { rect: [f64; 4], key: ImageKey },
    Gridline { line: [f64; 4] },
    Shape(Rc<ShapeItem>),
    Turned(Rc<Turned>),
}

#[derive(Debug)]
pub struct Turned {
    pub centre: [f64; 2],
    pub deg: f64,
    pub items: Vec<Item>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Layer {
    pub behind: bool,
    pub z: u32,
    pub order: usize,
}

#[derive(Debug, PartialEq)]
pub struct ShapeItem {
    pub rect: [f64; 4],
    pub rot: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub geom: Rc<crate::drawing::geom::Geom>,
    pub fill: crate::drawing::paint::Paint,
    pub line: crate::drawing::paint::ResolvedLine,
    pub effects: crate::drawing::paint::ResolvedEffects,
    pub behind: bool,
    pub float: u64,
    pub path: crate::drawing::ChildPath,
    pub picture: bool,
}

impl ShapeItem {
    pub fn hit(&self, x: f64, y: f64, slop: f64) -> bool {
        use crate::drawing::paint::Paint;
        let stroked = !matches!(self.line.paint, Paint::None);
        self.geom.hit(self.to_local([x, y]), !matches!(self.fill, Paint::None), stroked.then_some(self.line.width / 2.0 + slop))
    }

    pub fn to_local(&self, p: [f64; 2]) -> [f64; 2] {
        let [x0, y0, x1, y1] = self.rect;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (s, c) = (-self.rot).to_radians().sin_cos();
        let (dx, dy) = (p[0] - cx, p[1] - cy);
        let (mut qx, mut qy) = (dx * c - dy * s, dx * s + dy * c);
        if self.flip_h {
            qx = -qx;
        }
        if self.flip_v {
            qy = -qy;
        }
        [qx + cx - x0, qy + cy - y0]
    }

    pub fn to_page(&self, p: [f64; 2]) -> [f64; 2] {
        let [x0, y0, x1, y1] = self.rect;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (mut dx, mut dy) = (p[0] + x0 - cx, p[1] + y0 - cy);
        if self.flip_h {
            dx = -dx;
        }
        if self.flip_v {
            dy = -dy;
        }
        let (s, c) = self.rot.to_radians().sin_cos();
        [cx + dx * c - dy * s, cy + dx * s + dy * c]
    }

    pub fn moved(&self, dx: f64, dy: f64) -> ShapeItem {
        let r = self.rect;
        ShapeItem {
            rect: [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy],
            rot: self.rot,
            flip_h: self.flip_h,
            flip_v: self.flip_v,
            geom: self.geom.clone(),
            fill: self.fill.clone(),
            line: self.line.clone(),
            effects: self.effects.clone(),
            behind: self.behind,
            float: self.float,
            path: self.path.clone(),
            picture: self.picture,
        }
    }
}

impl Item {
    pub fn rect(rect: [f64; 4], colour: u32) -> Item {
        Item::Rect { rect, colour }
    }

    pub fn top(&self) -> f64 {
        match self {
            Item::Glyphs(r) => r.baseline,
            Item::Rect { rect, .. } | Item::Image { rect, .. } => rect[1],
            Item::Gridline { line } => line[1],
            Item::Shape(s) => s.rect[1],
            Item::Turned(t) => t.items.first().map_or(t.centre[1], Item::top),
        }
    }
}

#[derive(Debug)]
pub struct LineFragment {
    pub items: Box<[Item]>,
    pub pitch: f64,
    pub baseline: f64,
    /// The spacing a multiple adds below the line's text, within its pitch.
    pub below: f64,
    pub atoms: Box<[Atom]>,
    pub changed: bool,
    pub mark: Option<Unit>,
    pub texts: Box<[([f64; 4], crate::docx::Picture)]>,
    pub expansions: Box<[([f64; 4], Rc<super::expansion::Expansion>)]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atom {
    pub x0: f64,
    pub x1: f64,
    pub para: ParaId,
    pub unit: Unit,
    pub brk: bool,
}

#[derive(Clone, Debug)]
pub struct PlacedLine {
    pub x: f64,
    pub y: f64,
    pub line: Rc<LineFragment>,
}

#[derive(Clone, Debug)]
pub struct ParaFragment {
    pub para: ParaId,
    pub lines: Box<[PlacedLine]>,
    pub line_numbers: Box<[Box<[Item]>]>,
}

#[derive(Clone, Debug)]
pub struct CellFragment {
    pub cell: usize,
    pub content: Box<[Child]>,
    pub place: CellPlace,
}

#[derive(Clone, Debug)]
pub struct CellPlace {
    pub index: usize,
    pub col: usize,
    pub span: usize,
    pub rect: [f64; 4],
    pub clip: Option<f64>,
    pub v_merge: VMerge,
    pub hidden: Box<[(ParaId, Unit, Unit)]>,
    pub closing: Option<(ParaId, [f64; 3])>,
}

impl CellPlace {
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

#[derive(Clone, Debug)]
pub struct RowFragment {
    pub row: usize,
    pub rect: [f64; 4],
    pub top: f64,
    pub edges: Box<[(f64, usize)]>,
    pub background: Box<[Item]>,
    pub cells: Box<[CellFragment]>,
    pub borders: Box<[Item]>,
    pub repeat: bool,
    pub changed: bool,
}

#[derive(Clone, Debug)]
pub struct TableFragment {
    pub table: TableId,
    pub rows: Box<[RowFragment]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoxKind {
    Body,
    Column,
    Notes,
    Note { kind: NoteKind, id: i64 },
    Separator,
    Float { anchor: ParaId, float: FloatId, rect: [f64; 4], layer: Layer },
    Header { story: String },
    Footer { story: String },
}

#[derive(Clone, Debug)]
pub enum Child {
    Box(BoxFragment),
    Para(ParaFragment),
    Table(TableFragment),
    Line { para: Option<ParaId>, placed: PlacedLine },
    Items(Box<[Item]>),
    Group { offset: (f64, f64), children: Rc<[Child]> },
}

#[derive(Clone, Debug)]
pub struct BoxFragment {
    pub kind: BoxKind,
    pub id: Option<FragmentId>,
    pub offset: (f64, f64),
    pub size: (f64, f64),
    pub children: Box<[Child]>,
}

#[derive(Debug)]
pub struct PageFragment {
    pub id: FragmentId,
    pub w: f64,
    pub h: f64,
    pub body_area: Option<(f64, f64)>,
    pub children: Box<[PageChild]>,
    /// The page's borders, drawn before its stories.
    pub borders: Box<[Item]>,
}

#[derive(Clone, Debug)]
pub enum PageChild {
    Body(Rc<BoxFragment>),
    Header(BoxFragment),
    Footer(BoxFragment),
}

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

    pub fn truncate(&mut self, len: usize) {
        self.children.truncate(len);
    }

    pub fn finish(self, kind: BoxKind, offset: (f64, f64), size: (f64, f64)) -> BoxFragment {
        BoxFragment { kind, id: None, offset, size, children: self.children.into_boxed_slice() }
    }
}
