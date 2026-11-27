//! Graphics: shapes, pictures and groups, with their geometry, paint and text bodies, and the theme their colours
//! resolve against

pub mod colour;
pub mod geom;
pub mod paint;
pub mod patterns;
pub mod text;
pub mod theme;
pub mod units;

use std::rc::Rc;

use geom::Geometry;
use paint::{BlipFill, EffectList, Fill, Line, StyleRefs};
use text::BodyProps;
use units::{Angle, Emu};

/// A shape's frame: its box, and how it is turned and flipped about the box's centre.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Xfrm {
    /// The box's top left, in its parent's coordinates.
    pub off: [Emu; 2],
    /// The box's width and height.
    pub ext: [Emu; 2],
    /// The clockwise turn.
    pub rot: Angle,
    /// Mirrored left to right.
    pub flip_h: bool,
    /// Mirrored top to bottom.
    pub flip_v: bool,
}

/// What a shape is beside its drawing: its id, name, descriptions and locks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NonVisual {
    /// Its id among the document's drawings.
    pub id: u32,
    /// Its name, as the selection pane lists it.
    pub name: String,
    /// Its alternative text.
    pub descr: Rc<str>,
    /// Its title (`title`): a short name for the object that screen readers read before its description.
    pub title: Rc<str>,
    /// Hidden: not drawn.
    pub hidden: bool,
    /// Its aspect ratio is kept when it is resized.
    pub lock_aspect: bool,
    /// Decorative: screen readers skip it.
    pub decorative: bool,
}

/// The text a shape holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ShapeText {
    /// None.
    #[default]
    None,
    /// A story of its own.
    Story,
    /// Its place in a chain of linked text boxes, whose text runs on from box to box.
    Linked {
        /// The chain's id.
        chain: u32,
        /// Its place in the chain.
        seq: u32,
    },
}

/// A shape drawn from a geometry: an autoshape, a line or a text box.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpShape {
    /// Its outline.
    pub geom: Geometry,
    /// Its own fill, over its style's.
    pub fill: Option<Fill>,
    /// Its own line, over its style's.
    pub line: Option<Line>,
    /// Its own effects, over its style's.
    pub effects: Option<EffectList>,
    /// The theme's fill, line, effect and font colour it takes.
    pub style: Option<StyleRefs>,
    /// How its text is laid out in it.
    pub body: BodyProps,
    /// Its text.
    pub text: ShapeText,
    /// The id of the chain of linked text boxes it heads.
    pub chain: Option<u32>,
    /// A text box: the shape is there for its text.
    pub text_box: bool,
    /// A connector: a line between shapes.
    pub connector: bool,
    /// It has 3-D properties, kept but not drawn.
    pub kept_3d: bool,
    /// A VML text path (`v:textpath`): the shape draws this text in place of its geometry.
    pub text_path: Option<Box<TextPath>>,
}

/// A VML text path: a string drawn as glyph outlines in the shape's fill and line, as Word's text watermarks and
/// WordArt are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextPath {
    /// The text.
    pub string: String,
    /// Its font family.
    pub family: String,
    /// `font-size`, points.
    pub size: f64,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// `fitshape`: the text is stretched to fill the shape's box. Word draws no text without it.
    pub fit_shape: bool,
    /// `fitpath`: the text is scaled to the path's length. Word draws a stretched text the same with or without it.
    pub fit_path: bool,
    /// `trim`: the text fills the whole box, without the insets a text path keeps from its edges.
    pub trim: bool,
}

/// A picture.
#[derive(Clone, Debug, PartialEq)]
pub struct PicShape {
    /// Its outline, which clips the picture.
    pub geom: Geometry,
    /// The picture, and how it fills the shape.
    pub blip: BlipFill,
    /// Its line.
    pub line: Option<Line>,
    /// Its effects.
    pub effects: Option<EffectList>,
    /// It has 3-D properties, kept but not drawn.
    pub kept_3d: bool,
}

/// A group of shapes or a drawing canvas, its children placed in coordinates of its own.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupShape<S = ()> {
    /// The top left of the children's coordinates, at the group's box's top left.
    pub child_off: [Emu; 2],
    /// The size of the children's coordinates, the group's box's size.
    pub child_ext: [Emu; 2],
    /// The fill its children with a group fill take.
    pub fill: Option<Fill>,
    /// Its shapes, back to front.
    pub children: Vec<Shape<S>>,
    /// A drawing canvas.
    pub canvas: bool,
}

/// A graphic the model doesn't hold (a chart, a diagram without its drawing, a graphic frame): kept, not drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct RawGraphic {
    /// What it is: its graphic data's URI, or the element it is.
    pub uri: String,
}

/// What a shape is.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeKind<S = ()> {
    /// A shape drawn from a geometry.
    Sp(Box<SpShape>),
    /// A picture.
    Pic(Box<PicShape>),
    /// A group.
    Group(GroupShape<S>),
    /// A graphic kept, not drawn.
    Raw(RawGraphic),
}

/// A shape, picture or group, with what its reader keeps of where it was read from: `S`, `()` for nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape<S = ()> {
    /// Its id, name and descriptions.
    pub nv: NonVisual,
    /// Its frame.
    pub xfrm: Xfrm,
    /// What it is.
    pub kind: ShapeKind<S>,
    /// Where it was read from.
    pub spans: S,
}

/// A group child's place in its group tree: its index in each group, from the top down.
pub type ChildPath = Vec<u16>;

/// A shape at the bottom of a group tree, placed on the page.
#[derive(Clone, Debug)]
pub struct Leaf<'a, S = ()> {
    /// Its place in the tree.
    pub path: ChildPath,
    /// Its box, in points: left, top, right, bottom.
    pub rect: [f64; 4],
    /// Its clockwise turn in degrees, its groups' and its own.
    pub rot: f64,
    /// Mirrored left to right, its groups' flips and its own.
    pub flip_h: bool,
    /// Mirrored top to bottom, its groups' flips and its own.
    pub flip_v: bool,
    /// The shape.
    pub shape: &'a Shape<S>,
    /// The fill of its nearest group with one, for a group fill.
    pub group_fill: Option<&'a Fill>,
}

/// A shape to insert.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NewShape {
    /// A shape of a preset geometry, in the theme's shape style.
    Shape(geom::Preset),
    /// A line of a preset geometry, in the theme's line style.
    Line(geom::Preset),
    /// A text box: white, with a thin black line.
    TextBox,
}

impl<S: Default> Shape<S> {
    /// A new shape, line or text box named `name`, `w` by `h`, styled as Word inserts one.
    pub fn new(kind: NewShape, w: Emu, h: Emu, name: &str) -> Shape<S> {
        use colour::{Colour, ColourMod, SchemeSlot};
        use text::VAnchor;
        let accent = || Some(Colour::scheme(SchemeSlot::Accent(1)));
        let preset = |p| Geometry::Preset { preset: p, adj: Vec::new() };
        let sp = match kind {
            NewShape::Shape(p) => SpShape {
                geom: preset(p),
                style: Some(StyleRefs {
                    line: Some((2, Some(Colour::scheme(SchemeSlot::Accent(1)).with(ColourMod::Shade(units::Pct(15000)))))),
                    fill: Some((1, accent())),
                    effect: Some((0, accent())),
                    font: Some(Colour::scheme(SchemeSlot::Lt1)),
                }),
                body: BodyProps { anchor: VAnchor::Centre, ..BodyProps::default() },
                ..SpShape::default()
            },
            NewShape::Line(p) => SpShape {
                geom: preset(p),
                style: Some(StyleRefs {
                    line: Some((1, accent())),
                    fill: Some((0, accent())),
                    effect: Some((0, accent())),
                    font: Some(Colour::scheme(SchemeSlot::Tx1)),
                }),
                connector: true,
                ..SpShape::default()
            },
            NewShape::TextBox => SpShape {
                geom: preset(geom::Preset::Rect),
                fill: Some(Fill::Solid(Colour::srgb(0xFF_FFFF))),
                line: Some(Line { width: Some(Emu(6350)), fill: Some(Fill::Solid(Colour::srgb(0))), ..Line::default() }),
                text: ShapeText::Story,
                text_box: true,
                ..SpShape::default()
            },
        };
        Shape {
            nv: NonVisual { name: name.to_string(), ..NonVisual::default() },
            xfrm: Xfrm { ext: [w, h], ..Xfrm::default() },
            kind: ShapeKind::Sp(Box::new(sp)),
            spans: S::default(),
        }
    }

    /// A new picture of image `image`, `w` by `h`, stretched over its box.
    pub fn picture(image: usize, w: Emu, h: Emu) -> Shape<S> {
        let blip = paint::BlipFill {
            image: Some(image),
            src: [units::Pct(0); 4],
            mode: paint::BlipMode::Stretch([units::Pct(0); 4]),
            adjust: Vec::new(),
            rot_with_shape: true,
        };
        Shape {
            nv: NonVisual { name: "Picture".into(), lock_aspect: true, ..NonVisual::default() },
            xfrm: Xfrm { ext: [w, h], ..Xfrm::default() },
            kind: ShapeKind::Pic(Box::new(PicShape {
                geom: Geometry::Preset { preset: geom::Preset::Rect, adj: Vec::new() },
                blip,
                line: None,
                effects: None,
                kept_3d: false,
            })),
            spans: S::default(),
        }
    }
}

impl<S> Shape<S> {
    /// Moves every image index in the shape tree (blips, blip fills, line fills) up by `base`.
    pub fn rebase_images(&mut self, base: usize) {
        fn blip(b: &mut BlipFill, base: usize) {
            b.image = b.image.map(|k| k + base);
        }
        fn fill(f: Option<&mut Fill>, base: usize) {
            if let Some(Fill::Blip(b)) = f {
                blip(b, base);
            }
        }
        fn line(l: Option<&mut Line>, base: usize) {
            fill(l.and_then(|l| l.fill.as_mut()), base);
        }
        match &mut self.kind {
            ShapeKind::Sp(s) => {
                fill(s.fill.as_mut(), base);
                line(s.line.as_mut(), base);
            }
            ShapeKind::Pic(p) => {
                blip(&mut p.blip, base);
                line(p.line.as_mut(), base);
            }
            ShapeKind::Group(g) => {
                fill(g.fill.as_mut(), base);
                g.children.iter_mut().for_each(|c| c.rebase_images(base));
            }
            ShapeKind::Raw(_) => {}
        }
    }

    /// The shape drawn from a geometry it is.
    pub fn sp(&self) -> Option<&SpShape> {
        match &self.kind {
            ShapeKind::Sp(s) => Some(s),
            _ => None,
        }
    }

    /// The shape drawn from a geometry it is, to change.
    pub fn sp_mut(&mut self) -> Option<&mut SpShape> {
        match &mut self.kind {
            ShapeKind::Sp(s) => Some(s),
            _ => None,
        }
    }

    /// The picture it is.
    pub fn pic(&self) -> Option<&PicShape> {
        match &self.kind {
            ShapeKind::Pic(p) => Some(p),
            _ => None,
        }
    }

    /// The group it is.
    pub fn group(&self) -> Option<&GroupShape<S>> {
        match &self.kind {
            ShapeKind::Group(g) => Some(g),
            _ => None,
        }
    }

    /// Its geometry: a shape's or a picture's.
    pub fn geometry(&self) -> Option<&Geometry> {
        match &self.kind {
            ShapeKind::Sp(s) => Some(&s.geom),
            ShapeKind::Pic(p) => Some(&p.geom),
            ShapeKind::Group(_) | ShapeKind::Raw(_) => None,
        }
    }

    /// The shape at `path` in its group tree.
    pub fn at(&self, path: &[u16]) -> Option<&Shape<S>> {
        match path.split_first() {
            None => Some(self),
            Some((i, rest)) => self.group()?.children.get(*i as usize)?.at(rest),
        }
    }

    /// The shape at `path` in its group tree, to change.
    pub fn at_mut(&mut self, path: &[u16]) -> Option<&mut Shape<S>> {
        match path.split_first() {
            None => Some(self),
            Some((i, rest)) => match &mut self.kind {
                ShapeKind::Group(g) => g.children.get_mut(*i as usize)?.at_mut(rest),
                _ => None,
            },
        }
    }

    /// Every shape in its group tree with its path, itself first, each group before its children.
    pub fn walk(&self) -> Vec<(ChildPath, &Shape<S>)> {
        fn go<'a, S>(s: &'a Shape<S>, path: &mut ChildPath, out: &mut Vec<(ChildPath, &'a Shape<S>)>) {
            out.push((path.clone(), s));
            if let Some(g) = s.group() {
                for (i, c) in g.children.iter().enumerate() {
                    path.push(i as u16);
                    go(c, path, out);
                    path.pop();
                }
            }
        }
        let mut out = Vec::new();
        go(self, &mut Vec::new(), &mut out);
        out
    }

    /// The shapes at the bottom of its group tree, back to front, placed with it at `rect` (points).
    pub fn leaves(&self, rect: [f64; 4]) -> Vec<Leaf<'_, S>> {
        let mut out = Vec::new();
        let rot = self.xfrm.rot.normalized().degrees();
        self.collect(rect, rot, self.xfrm.flip_h, self.xfrm.flip_v, None, &mut Vec::new(), &mut out);
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn collect<'a>(
        &'a self,
        rect: [f64; 4],
        rot: f64,
        flip_h: bool,
        flip_v: bool,
        group_fill: Option<&'a Fill>,
        path: &mut ChildPath,
        out: &mut Vec<Leaf<'a, S>>,
    ) {
        let Some(g) = self.group() else {
            out.push(Leaf { path: path.clone(), rect, rot, flip_h, flip_v, shape: self, group_fill });
            return;
        };
        let fill = g.fill.as_ref().or(group_fill);
        for (i, c) in g.children.iter().enumerate() {
            let (r, rot, flip_h, flip_v) = g.child_frame(c, rect, rot, flip_h, flip_v);
            path.push(i as u16);
            c.collect(r, rot, flip_h, flip_v, fill, path, out);
            path.pop();
        }
    }
}

/// A shape placed on the page: its box in points (left, top, right, bottom), its clockwise turn in degrees, and
/// whether it is mirrored left to right and top to bottom.
pub type Frame = ([f64; 4], f64, bool, bool);

impl<S> Shape<S> {
    /// Where the shape at `path` in its group tree is, with it placed at `rect` (points).
    pub fn frame_at(&self, rect: [f64; 4], path: &[u16]) -> Option<Frame> {
        let mut f = (rect, self.xfrm.rot.normalized().degrees(), self.xfrm.flip_h, self.xfrm.flip_v);
        let mut s = self;
        for &i in path {
            let g = s.group()?;
            s = g.children.get(i as usize)?;
            f = g.child_frame(s, f.0, f.1, f.2, f.3);
        }
        Some(f)
    }
}

impl<S> GroupShape<S> {
    /// Points per child coordinate across and down, with the group placed at `rect` (points).
    pub fn scale(&self, rect: [f64; 4]) -> [f64; 2] {
        let scale = |len: f64, ext: Emu| if ext.0 > 0 { len / ext.to_pt() } else { 1.0 };
        [scale(rect[2] - rect[0], self.child_ext[0]), scale(rect[3] - rect[1], self.child_ext[1])]
    }

    fn child_frame(&self, c: &Shape<S>, rect: [f64; 4], rot: f64, flip_h: bool, flip_v: bool) -> Frame {
        let [x0, y0, x1, y1] = rect;
        let [sx, sy] = self.scale(rect);
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (sin, cos) = rot.to_radians().sin_cos();
        let w = c.xfrm.ext[0].to_pt() * sx;
        let h = c.xfrm.ext[1].to_pt() * sy;
        let mut px = x0 + (c.xfrm.off[0] - self.child_off[0]).to_pt() * sx + w / 2.0;
        let mut py = y0 + (c.xfrm.off[1] - self.child_off[1]).to_pt() * sy + h / 2.0;
        if flip_h {
            px = 2.0 * cx - px;
        }
        if flip_v {
            py = 2.0 * cy - py;
        }
        let (dx, dy) = (px - cx, py - cy);
        let (px, py) = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
        let own = c.xfrm.rot.normalized().degrees();
        let child_rot = if flip_h != flip_v { -own } else { own };
        let r = (rot + child_rot).rem_euclid(360.0);
        ([px - w / 2.0, py - h / 2.0, px + w / 2.0, py + h / 2.0], r, c.xfrm.flip_h != flip_h, c.xfrm.flip_v != flip_v)
    }

    /// Grows the child box to take in every child, or with `tight` fits it to them, and `xfrm` with it: how far each
    /// edge of the frame moved out (left, top, right, bottom). A flipped axis shows the child box mirrored, so the frame
    /// moves the other way along it and the children stay where they show.
    pub fn fit_children(&mut self, xfrm: &mut Xfrm, tight: bool) -> [i64; 4] {
        let (mut lo, mut hi) = ([i64::MAX; 2], [i64::MIN; 2]);
        for c in &self.children {
            for a in 0..2 {
                lo[a] = lo[a].min(c.xfrm.off[a].0);
                hi[a] = hi[a].max(c.xfrm.off[a].0 + c.xfrm.ext[a].0);
            }
        }
        let mut grow = [0; 4];
        if self.children.is_empty() {
            return grow;
        }
        for a in 0..2 {
            let (o, e) = (self.child_off[a].0, self.child_ext[a].0);
            let (no, ne) = if tight { (lo[a], hi[a]) } else { (lo[a].min(o), hi[a].max(o + e)) };
            if (no, ne) == (o, o + e) {
                continue;
            }
            let k = if e > 0 { xfrm.ext[a].0 as f64 / e as f64 } else { 1.0 };
            let (mut before, mut after) = (((o - no) as f64 * k).round() as i64, ((ne - o - e) as f64 * k).round() as i64);
            if [xfrm.flip_h, xfrm.flip_v][a] {
                std::mem::swap(&mut before, &mut after);
            }
            xfrm.off[a].0 -= before;
            xfrm.ext[a].0 += before + after;
            self.child_off[a].0 = no;
            self.child_ext[a].0 = ne - no;
            (grow[a], grow[a + 2]) = (before, after);
        }
        grow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_shape(off: [i64; 2], ext: [i64; 2]) -> Shape {
        Shape {
            nv: NonVisual::default(),
            xfrm: Xfrm { off: off.map(Emu), ext: ext.map(Emu), ..Xfrm::default() },
            kind: ShapeKind::Sp(Box::default()),
            spans: (),
        }
    }

    fn group(off: [i64; 2], ext: [i64; 2], ch_off: [i64; 2], ch_ext: [i64; 2], children: Vec<Shape>) -> Shape {
        Shape {
            nv: NonVisual::default(),
            xfrm: Xfrm { off: off.map(Emu), ext: ext.map(Emu), ..Xfrm::default() },
            kind: ShapeKind::Group(GroupShape { child_off: ch_off.map(Emu), child_ext: ch_ext.map(Emu), fill: None, children, canvas: false }),
            spans: (),
        }
    }

    const PT: i64 = 12700;

    #[test]
    fn groups_map_their_children_and_nested_groups_compose() {
        let inner = group([50 * PT, 0], [50 * PT, 60 * PT], [0, 0], [50 * PT, 40 * PT], vec![rect_shape([0, 20 * PT], [10 * PT, 20 * PT])]);
        let g = group([0, 0], [200 * PT, 100 * PT], [0, 0], [100 * PT, 100 * PT], vec![rect_shape([10 * PT, 10 * PT], [20 * PT, 30 * PT]), inner]);
        let leaves = g.leaves([72.0, 100.0, 272.0, 200.0]);
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].rect, [92.0, 110.0, 132.0, 140.0]);
        assert_eq!(leaves[0].path, vec![0]);
        assert_eq!(leaves[1].rect, [172.0, 130.0, 192.0, 160.0]);
        assert_eq!(leaves[1].path, vec![1, 0]);
    }

    #[test]
    fn a_tight_fit_keeps_a_flipped_groups_children_where_they_show() {
        for (flip_h, flip_v) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut g = group(
                [0, 0],
                [100 * PT, 100 * PT],
                [0, 0],
                [100 * PT, 100 * PT],
                vec![rect_shape([10 * PT, 20 * PT], [20 * PT, 30 * PT]), rect_shape([0, 0], [100 * PT, 100 * PT])],
            );
            (g.xfrm.flip_h, g.xfrm.flip_v) = (flip_h, flip_v);
            let before = g.leaves([0.0, 0.0, 100.0, 100.0])[0].rect;
            let Shape { xfrm, kind: ShapeKind::Group(gs), .. } = &mut g else { panic!("a group") };
            gs.children.pop();
            let grow = gs.fit_children(xfrm, true);
            let pt = |e: i64| e as f64 / PT as f64;
            let rect = [-pt(grow[0]), -pt(grow[1]), 100.0 + pt(grow[2]), 100.0 + pt(grow[3])];
            let after = g.leaves(rect)[0].rect;
            assert!(before.iter().zip(after).all(|(a, b)| (a - b).abs() < 1e-9), "{flip_h} {flip_v}: {before:?} {after:?}");
        }
    }

    #[test]
    fn a_rotated_flipped_group_turns_and_mirrors_its_children() {
        let mut g = group([0, 0], [100 * PT, 100 * PT], [0, 0], [100 * PT, 100 * PT], vec![rect_shape([0, 0], [20 * PT, 20 * PT])]);
        g.xfrm.rot = Angle(90 * 60000);
        let l = &g.leaves([0.0, 0.0, 100.0, 100.0])[0];
        assert!((l.rect[0] - 80.0).abs() < 1e-9 && (l.rect[1] - 0.0).abs() < 1e-9);
        assert_eq!(l.rot, 90.0);
        g.xfrm.flip_h = true;
        let l = &g.leaves([0.0, 0.0, 100.0, 100.0])[0];
        assert!((l.rect[0] - 80.0).abs() < 1e-9 && (l.rect[1] - 80.0).abs() < 1e-9);
        assert!(l.flip_h);
    }
}
