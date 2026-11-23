//! Layout fragments

use std::rc::Rc;

use crate::docx::NoteKind;
use crate::font::Face;

use crate::docx::VMerge;

use super::ids::{FloatId, FragmentId, ParaId, TableId, Unit};
use super::units::EPS;

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
    /// How the glyphs paint when not filled in `colour` alone; `None` for ordinary text.
    pub paint: Option<Rc<GlyphPaint>>,
}

/// A glyph run's paint: its fill, its glyphs' outline and offset copies drawn under it, and Word 2010's text effects.
#[derive(Debug, PartialEq)]
pub struct GlyphPaint {
    pub fill: GlyphFill,
    /// The outline's width in points and its colour.
    pub outline: Option<(f64, u32)>,
    /// Copies of the run drawn before it, each moved by (dx, dy) points, in a colour of its own.
    pub under: Box<[(f64, f64, u32)]>,
    /// The run's text effects, resolved; `None` without any.
    pub effects: Option<Box<TextEffectPaint>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GlyphFill {
    None,
    Solid(u32),
    /// A text fill (`w14:textFill`): a solid with its alpha, or a gradient across the run's box (its advance wide,
    /// from the face's ascent to its descent).
    Paint(Box<crate::drawing::paint::Paint>),
}

/// A run's text effects in points, degrees and colours: the outline stroked over the fill, and the shadow,
/// reflection and glow drawn under it, in the run's box as [`GlyphFill::Paint`] has it.
#[derive(Debug, PartialEq)]
pub struct TextEffectPaint {
    pub outline: Option<crate::drawing::paint::ResolvedLine>,
    pub shadow: Option<crate::drawing::paint::ResolvedShadow>,
    /// The glow's radius and colour.
    pub glow: Option<(f64, crate::drawing::colour::Rgba)>,
    pub reflection: Option<crate::drawing::paint::ResolvedReflection>,
}

fn hash_paint<H: std::hash::Hasher>(p: &crate::drawing::paint::Paint, h: &mut H) {
    use crate::drawing::paint::Paint;
    use std::hash::Hash;
    std::mem::discriminant(p).hash(h);
    match p {
        Paint::Solid(c) => c.hash(h),
        Paint::Gradient { stops, kind, tile, flip, rot_with_shape } => {
            stops.iter().for_each(|(t, c)| (t.to_bits(), c).hash(h));
            (kind, tile, flip, rot_with_shape).hash(h);
        }
        Paint::Pattern { preset, fg, bg } => (preset, fg, bg).hash(h),
        Paint::Hatch { fg, bg, .. } => (fg, bg).hash(h),
        Paint::None | Paint::Picture { .. } => {}
    }
}

fn hash_bits<H: std::hash::Hasher>(v: &[f64], h: &mut H) {
    use std::hash::Hash;
    v.iter().for_each(|x| x.to_bits().hash(h));
}

impl std::hash::Hash for TextEffectPaint {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        if let Some(l) = &self.outline {
            1u8.hash(h);
            hash_bits(&[l.width], h);
            hash_paint(&l.paint, h);
            (&l.dash, l.compound, l.cap, l.join, l.inset, l.head.is_some(), l.tail.is_some()).hash(h);
        }
        if let Some(s) = &self.shadow {
            2u8.hash(h);
            hash_bits(&[s.blur, s.dist, s.dir, s.scale[0], s.scale[1], s.skew[0], s.skew[1]], h);
            (s.align, s.colour, s.rot_with_shape).hash(h);
        }
        if let Some((r, c)) = self.glow {
            3u8.hash(h);
            hash_bits(&[r], h);
            c.hash(h);
        }
        if let Some(r) = &self.reflection {
            4u8.hash(h);
            let v = [r.blur, r.dist, r.dir, r.fade_dir, r.scale[0], r.scale[1], r.skew[0], r.skew[1], r.start.0, r.start.1, r.end.0, r.end.1];
            hash_bits(&v, h);
            (r.align, r.rot_with_shape).hash(h);
        }
    }
}

impl std::hash::Hash for GlyphPaint {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        match &self.fill {
            GlyphFill::None => 0u8.hash(h),
            GlyphFill::Solid(c) => (1u8, c).hash(h),
            GlyphFill::Paint(p) => {
                2u8.hash(h);
                hash_paint(p, h);
            }
        }
        self.outline.map(|(w, c)| (w.to_bits(), c)).hash(h);
        self.under.iter().for_each(|&(dx, dy, c)| (dx.to_bits(), dy.to_bits(), c).hash(h));
        self.effects.hash(h);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey {
    pub index: Option<usize>,
    pub generation: u32,
}

#[derive(Clone, Debug)]
pub enum Item {
    Glyphs(GlyphRun),
    /// A filled rect; `snap`: aliased, its edges on whole device pixels (shading, so fills that meet leave no seam);
    /// `dash`: a border line's dashes, along its longer side, starting `phase` points into the pattern.
    Rect {
        rect: [f64; 4],
        colour: u32,
        snap: bool,
        dash: Dash,
        phase: f32,
    },
    Image {
        rect: [f64; 4],
        key: ImageKey,
        of: PictureOf,
    },
    Gridline {
        line: [f64; 4],
    },
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

/// The picture an image or shape item draws, in the model: an inline picture (its paragraph and unit) or a float.
/// Drawing doesn't read it and the layout dump doesn't show it; a tagged PDF finds a figure's picture by it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PictureOf {
    #[default]
    None,
    Inline(ParaId, Unit),
    Float(FloatId),
}

#[derive(Clone, Debug, PartialEq)]
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
    /// Its line is a document ink colour (a border or a check box), mapped by the appearance like a rect's.
    pub ink: bool,
    /// Drawn for text or a table (an underline's wave, a check box, a border's stroke), not an object: never hit,
    /// selected, snapped to or taken for an inline picture.
    pub decoration: bool,
    /// Filled aliased, its edges on whole device pixels: shading, so fills that meet leave no seam.
    pub snap: bool,
    /// The page y below which an inline picture's reflection is clipped: the bottom of its paragraph, spacing after
    /// included.
    pub clip_bottom: Option<f64>,
    /// The float holding the text of an inline text box, which `float` (0 for inline) doesn't name; 0 for none.
    pub text_float: u64,
    /// The text of a floated shape with a reflection, which the reflection holds: its glyphs, in the shape's frame.
    pub text: Option<ShapeText>,
    pub of: PictureOf,
    /// A text path's glyphs, drawn in place of the geometry's outline (which stays the shape's box, for hits).
    pub text_path: Option<Rc<TextPathItem>>,
}

/// The glyphs a shape's reflection holds, shared between the copies of the shape layout makes; equal when they are the
/// same.
#[derive(Clone, Debug)]
pub struct ShapeText(pub Rc<[Clipped]>);

/// An item with the box it is clipped to, if any.
pub type Clipped = (Item, Option<[f64; 4]>);

impl PartialEq for ShapeText {
    fn eq(&self, other: &ShapeText) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A text path's glyphs, drawn as their outlines stretched so that their ink fills `fit`.
#[derive(Debug)]
pub struct TextPathItem {
    /// Runs of glyphs, each in one face: glyph ids with the x of each glyph's origin, in ems along one baseline.
    pub runs: Box<[(Rc<Face>, Box<[(u32, f64)]>)]>,
    /// The box the ink fills, in the shape's own box (from its top left).
    pub fit: [f64; 4],
}

impl PartialEq for TextPathItem {
    fn eq(&self, o: &TextPathItem) -> bool {
        self.fit == o.fit && self.runs.len() == o.runs.len() && self.runs.iter().zip(&o.runs).all(|(a, b)| Rc::ptr_eq(&a.0, &b.0) && a.1 == b.1)
    }
}

impl ShapeItem {
    /// The bytes it holds: itself, its geometry's paths and its child path (a cache's estimate).
    pub fn bytes(&self) -> usize {
        use std::mem::size_of;
        let g = &self.geom;
        size_of::<ShapeItem>()
            + size_of::<crate::drawing::geom::Geom>()
            + g.paths
                .iter()
                .map(|p| size_of::<crate::drawing::geom::GeomPath>() + p.segs.len() * size_of::<crate::drawing::geom::Seg>())
                .sum::<usize>()
            + self.path.len() * size_of::<u16>()
    }

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

    /// This shape with its reflection clipped at the page y `y`.
    pub fn clipped_to(&self, y: f64) -> ShapeItem {
        ShapeItem { clip_bottom: Some(y), ..self.moved(0.0, 0.0) }
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
            ink: self.ink,
            decoration: self.decoration,
            snap: self.snap,
            clip_bottom: self.clip_bottom.map(|y| y + dy),
            text_float: self.text_float,
            text: self.text.as_ref().map(|t| {
                ShapeText(t.0.iter().map(|(i, c)| (translated(i, dx, dy), c.map(|c| [c[0] + dx, c[1] + dy, c[2] + dx, c[3] + dy]))).collect())
            }),
            of: self.of,
            text_path: self.text_path.clone(),
        }
    }
}

/// A border line's dashes, drawn by the renderer in units of the line's width on whole device pixels, so they keep
/// their size in pixels at any zoom as Word's do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Dash {
    #[default]
    Solid,
    Dotted,
    Dashed,
    DotDash,
    DotDotDash,
    DashSmallGap,
}

impl Dash {
    /// An `ST_Border` style's dashes; solid for any other style.
    pub fn of(style: &str) -> Dash {
        match style {
            "dotted" => Dash::Dotted,
            "dashed" => Dash::Dashed,
            "dotDash" => Dash::DotDash,
            "dotDotDash" => Dash::DotDotDash,
            "dashSmallGap" => Dash::DashSmallGap,
            _ => Dash::Solid,
        }
    }

    /// On and off lengths in line widths, as Word draws them on screen and in its PDFs; empty for solid.
    pub fn pattern(self) -> &'static [u8] {
        match self {
            Dash::Solid => &[],
            Dash::Dotted => &[1, 1],
            Dash::Dashed => &[4, 4],
            Dash::DotDash => &[7, 3, 3, 3],
            Dash::DotDotDash => &[6, 2, 2, 2, 2, 2],
            Dash::DashSmallGap => &[4, 1],
        }
    }
}

impl Item {
    pub fn rect(rect: [f64; 4], colour: u32) -> Item {
        Item::Rect { rect, colour, snap: false, dash: Dash::Solid, phase: 0.0 }
    }

    /// Shading's flat fill: an aliased rect.
    pub fn fill(rect: [f64; 4], colour: u32) -> Item {
        Item::Rect { rect, colour, snap: true, dash: Dash::Solid, phase: 0.0 }
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

#[derive(Clone, Debug)]
pub struct LineFragment {
    pub items: Box<[Item]>,
    pub pitch: f64,
    pub baseline: f64,
    /// The spacing a multiple adds below the line's text, within its pitch.
    pub below: f64,
    /// The space after a joined paragraph's part shown on its own, below the line within its pitch.
    pub after: f64,
    pub atoms: Box<[Atom]>,
    pub changed: bool,
    pub mark: Option<Unit>,
    pub texts: Box<[([f64; 4], crate::docx::Picture)]>,
    pub expansions: Box<[([f64; 4], Rc<super::expansion::Expansion>)]>,
}

impl LineFragment {
    /// This line with the reflections of its inline pictures clipped `to` below its top, those that reach lower: the
    /// bottom of the part of the paragraph on the page the line is placed on.
    pub fn clipped(self: &Rc<Self>, to: f64) -> Rc<LineFragment> {
        let lower = |i: &Item| matches!(i, Item::Shape(s) if s.clip_bottom.is_some_and(|c| c > to));
        if !self.items.iter().any(lower) {
            return Rc::clone(self);
        }
        let items = self.items.iter().map(|i| match i {
            Item::Shape(s) if lower(i) => Item::Shape(Rc::new(s.clipped_to(to))),
            other => other.clone(),
        });
        Rc::new(LineFragment { items: items.collect(), ..(**self).clone() })
    }
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
            paint: r.paint.clone(),
        }),
        Item::Rect { rect, colour, snap, dash, phase } => Item::Rect { rect: r4(rect), colour: *colour, snap: *snap, dash: *dash, phase: *phase },
        Item::Image { rect, key, of } => Item::Image { rect: r4(rect), key: *key, of: *of },
        Item::Gridline { line } => Item::Gridline { line: r4(line) },
        Item::Shape(s) => Item::Shape(std::rc::Rc::new(s.moved(dx, dy))),
        Item::Turned(t) => Item::Turned(std::rc::Rc::new(super::fragment::Turned {
            centre: [t.centre[0] + dx, t.centre[1] + dy],
            deg: t.deg,
            items: t.items.iter().map(|i| translated(i, dx, dy)).collect(),
        })),
    }
}

fn item_down(it: &Item, dy: f64) -> Item {
    translated(it, 0.0, dy)
}

pub(crate) fn row_down(r: &RowFragment, dy: f64) -> RowFragment {
    RowFragment {
        row: r.row,
        rect: [r.rect[0], r.rect[1] + dy, r.rect[2], r.rect[3] + dy],
        top: r.top + dy,
        edges: r.edges.clone(),
        background: r.background.iter().map(|i| item_down(i, dy)).collect(),
        cells: r
            .cells
            .iter()
            .map(|c| super::fragment::CellFragment {
                cell: c.cell,
                content: c.content.iter().map(|x| child_down(x, &|_| dy)).collect(),
                place: c.place.moved_down(dy),
            })
            .collect(),
        borders: r.borders.iter().map(|i| item_down(i, dy)).collect(),
        repeat: r.repeat,
        changed: r.changed,
    }
}

pub(super) fn child_down(c: &Child, shift: &dyn Fn(f64) -> f64) -> Child {
    match c {
        Child::Para(p) => Child::Para(super::fragment::ParaFragment {
            para: p.para,
            lines: p.lines.iter().map(|l| PlacedLine { x: l.x, y: l.y + shift(l.y + EPS), line: l.line.clone() }).collect(),
            line_numbers: p.line_numbers.iter().map(|g| g.iter().map(|i| item_down(i, shift(i.top()))).collect()).collect(),
        }),
        Child::Line { para, placed } => {
            Child::Line { para: *para, placed: PlacedLine { x: placed.x, y: placed.y + shift(placed.y + EPS), line: placed.line.clone() } }
        }
        Child::Table(t) => Child::Table(super::fragment::TableFragment {
            table: t.table,
            rows: t.rows.iter().map(|r| row_down(r, shift(r.rect[1] + EPS))).collect(),
        }),
        Child::Items(items) => {
            Child::Items(items.iter().map(|i| item_down(i, shift(i.top() + if matches!(i, Item::Glyphs(_)) { 0.0 } else { EPS }))).collect())
        }
        Child::Group { offset, children } => Child::Group { offset: (offset.0, offset.1 + shift(offset.1 + EPS)), children: children.clone() },
        Child::Box(b) => {
            let mut b = b.clone();
            b.children = b.children.iter().map(|x| child_down(x, shift)).collect();
            Child::Box(b)
        }
    }
}
