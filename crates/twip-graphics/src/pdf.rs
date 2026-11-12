//! PDF: pictures written as the pages of a PDF with krilla (Laurenz Stampfl's PDF writer), which subsets and embeds the
//! fonts with ToUnicode maps, writes gradients as shadings, and keeps JPEG files as they are. Each page is a list of
//! pictures, each marked as a structure element's content or as an artifact; the structure tree, the outline (from
//! the headings) and the metadata come with them.
//!
//! What PDF has no operator for is composited as Skia's PDF backend does, with soft masks: a fill that clears
//! (`Clear`) or punches out (`DstOut`, solid paint) masks what its layer drew before it by the fill's inverse; a layer
//! composited destination-in masks what came before it by its alpha, and one composited source-in is drawn masked by
//! the alpha of what came before (which it replaces). Layers with filters or colour filters, and groups composited in
//! ways the masks don't cover (other Porter-Duff modes, or those modes under a clip of the group's own), are drawn by
//! the CPU backend at 300 dots per inch and embedded as images, as Skia rasterises what PDF can't express.

use std::hash::{Hash, Hasher};
use std::num::NonZeroU16;
use std::sync::Arc;

use krilla::color::rgb;
use krilla::configure::{Archival, Configuration, ConfigurationBuilder};
use krilla::destination::XyzDestination;
use krilla::geom::{Path, PathBuilder, Point as KPoint, Size, Transform as KTransform};
use krilla::image::{BitsPerComponent, CustomImage, Image, ImageColorspace};
use krilla::mask::{Mask, MaskType};
use krilla::metadata::{DateTime, Metadata};
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{
    Fill, FillRule as KRule, LineCap, LineJoin, LinearGradient, Paint as KPaint, Pattern, RadialGradient, SpreadMethod, Stop as KStop,
    Stroke as KStroke, SweepGradient,
};
use krilla::surface::Surface;
use krilla::tagging::{
    Artifact, ArtifactType, ContentTag, Identifier, ListNumbering, Node, SpanTag, TableHeaderScope, Tag, TagGroup, TagKind, TagTree,
};
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::{Document, SerializeSettings};
use kurbo::Affine;

use crate::Resources;
use crate::cover::{frame_path, rrect_path};
use crate::cpu::{self, Draw, Format, Target};
use crate::font::{FontId, embolden_em};
use crate::fx::FastMap;
use crate::geom::{Point, Rect, Transform};
use crate::image::{ImageFormat, ImageId};
use crate::paint::{BlendMode, Color, ColorFilter, Extend, ImagePaint, PremulColor, Sampling};
use crate::path::Verb;
use crate::scene::{BrushRec, ClipRec, Cmd, Lists, Picture, Recorder};

/// What a page's picture is in the document's structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Content of the structure element with this id ([`Element::content`]).
    Content(u32),
    /// Not content: a running header or footer and the like (`Pagination`), drawing that only lays out the page
    /// (`Layout`: borders, shading, rules), or anything else.
    Artifact(ArtifactKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactKind {
    Pagination,
    Layout,
    Other,
}

/// One page: its size in points, and its pictures (in points, y down from the top left), drawn in order.
#[derive(Clone, Debug)]
pub struct Page {
    pub width: f64,
    pub height: f64,
    pub parts: Vec<(Mark, Picture)>,
}

/// A structure element.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Paragraph,
    /// A heading of level 1 to 6.
    Heading(u8),
    List,
    ListItem,
    ListBody,
    Table,
    Row,
    /// A table header cell, heading its column.
    HeaderCell,
    Cell,
    /// A figure, with its alternative text.
    Figure(Option<String>),
}

/// A structure element: what it is, the content marked with its id (drawn before its kids), and its kids.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub kind: Kind,
    pub content: Option<u32>,
    pub kids: Vec<Element>,
}

/// An outline entry: a heading's level (1 to 6), its content id (the entry goes to where that is first drawn) and
/// its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Heading {
    pub level: u8,
    pub content: u32,
    pub title: String,
}

/// A UTC date and time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// The document's metadata, and whether it claims PDF/A-2b conformance (`archival`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub title: String,
    pub language: Option<String>,
    pub keywords: Vec<String>,
    pub creator: String,
    pub producer: String,
    pub created: Option<Date>,
    pub archival: bool,
}

/// A written PDF, and whether it claims PDF/A-2b (asked for, and what was drawn allows it).
#[derive(Clone, Debug)]
pub struct Written {
    pub bytes: Vec<u8>,
    pub archival: bool,
}

/// Why the PDF couldn't be written: krilla's reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the PDF couldn't be written: {}", self.0)
    }
}

impl std::error::Error for Error {}

/// Writes `pages` as a PDF, tagged with `structure` (the tree's top level, in reading order), with bookmarks from
/// `outline` and `info`'s metadata. A PDF/A claim that what was drawn breaks is dropped ([`Written::archival`]).
pub fn write(res: &Resources, pages: &[Page], structure: &[Element], outline: &[Heading], info: &Info) -> Result<Written, Error> {
    if info.archival {
        match write_with(res, pages, structure, outline, info, true) {
            Ok(bytes) => return Ok(Written { bytes, archival: true }),
            Err(krilla::error::KrillaError::Validation(errors)) if std::env::var_os("TWIP_GRAPHICS_PDF_VALIDATION").is_some() => {
                eprintln!("PDF/A: {errors:?}");
            }
            Err(krilla::error::KrillaError::Validation(_)) => {}
            Err(e) => return Err(Error(format!("{e:?}"))),
        }
    }
    let bytes = write_with(res, pages, structure, outline, info, false).map_err(|e| Error(format!("{e:?}")))?;
    Ok(Written { bytes, archival: false })
}

/// Device pixels per point of what is rasterised: 300 dots per inch.
const RASTER: f64 = 300.0 / 72.0;

fn write_with(
    res: &Resources,
    pages: &[Page],
    structure: &[Element],
    outline: &[Heading],
    info: &Info,
    archival: bool,
) -> krilla::error::KrillaResult<Vec<u8>> {
    let configuration = if archival {
        ConfigurationBuilder::new().with_archival_validator(Archival::A2_B).finish().unwrap_or_default()
    } else {
        Configuration::default()
    };
    let mut doc = Document::new_with(SerializeSettings { configuration, ..SerializeSettings::default() });
    let mut w = Writer::empty(res);
    // PDF/A forbids asking viewers to interpolate images.
    w.interpolate = !archival;
    for (index, page) in pages.iter().enumerate() {
        let Some(settings) = PageSettings::from_wh(page.width as f32, page.height as f32) else { continue };
        let mut p = doc.start_page_with(settings);
        let mut s = p.surface();
        w.page = kurbo::Rect::new(0.0, 0.0, page.width, page.height);
        for (mark, picture) in &page.parts {
            let id = match *mark {
                Mark::Content(c) => {
                    if let Some(b) = picture.bounds() {
                        w.firsts.entry(c).or_insert((index, b.origin()));
                    }
                    Some((c, s.start_tagged(ContentTag::Span(SpanTag::empty()))))
                }
                Mark::Artifact(kind) => {
                    let kind = match kind {
                        ArtifactKind::Pagination => ArtifactType::PaginationOther,
                        ArtifactKind::Layout => ArtifactType::Layout,
                        ArtifactKind::Other => ArtifactType::Other,
                    };
                    s.start_tagged(ContentTag::Artifact(Artifact::with_kind(kind)));
                    None
                }
            };
            w.picture(&mut s, picture, Affine::IDENTITY, Ink::Paint);
            s.end_tagged();
            if let Some((c, id)) = id {
                w.marks.entry(c).or_default().push(id);
            }
        }
        s.finish();
        p.finish();
    }
    let mut tree = TagTree::new().with_lang(info.language.clone());
    for e in structure {
        tree.push(w.element(e));
    }
    doc.set_tag_tree(tree);
    doc.set_outline(w.outline(outline));
    let mut m = Metadata::new().title(info.title.clone()).creator(info.creator.clone()).producer(info.producer.clone());
    if let Some(lang) = &info.language {
        m = m.language(lang.clone());
    }
    if !info.keywords.is_empty() {
        m = m.keywords(info.keywords.clone());
    }
    if let Some(d) = info.created {
        let date = DateTime::new(d.year).month(d.month).day(d.day).hour(d.hour).minute(d.minute).second(d.second);
        m = m.creation_date(date.utc_offset_hour(0).utc_offset_minute(0));
    }
    doc.set_metadata(m);
    doc.finish()
}

/// How a draw is painted: with its own paint, or (in a mask made from it) black, opaque or at its paint's alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ink {
    Paint,
    Black,
    BlackAlpha,
}

/// How a group of commands (a page's, or a layer's) is written: as it is, with soft masks for the Porter-Duff
/// modes they cover, or rasterised.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Plain,
    Masked,
    Raster,
}

/// A draw or a layer of a masked group, with the transforms and clips (the group's own) it is drawn under.
#[derive(Clone)]
struct Op<'p> {
    lists: &'p Lists,
    at: usize,
    context: Vec<Context<'p>>,
}

#[derive(Clone, Copy)]
enum Context<'p> {
    Transform(Affine),
    Clip(&'p Lists, ClipRec),
}

/// A masked group's drawing, folded: draws, and drawing masked by other drawing.
enum Piece {
    Op(usize),
    Masked { content: Vec<Piece>, mask: Vec<Piece>, kind: MaskKind },
}

#[derive(Clone, Copy)]
enum MaskKind {
    /// The mask's alpha.
    Alpha,
    /// The mask's alpha inside `bounds`, opaque outside (a destination-in layer with bounds).
    AlphaInside(kurbo::Rect),
    /// One minus the mask's alpha (painted black on white, as luminosity), with the mask's ink.
    Inverse(Ink),
}

/// An image as embedded: its stored size, its upright size, and the transform from stored pixels to upright ones.
#[derive(Clone)]
struct Embedded {
    image: Image,
    stored: (u32, u32),
    upright: (u32, u32),
    orient: Affine,
}

struct Writer<'r> {
    res: &'r Resources,
    cpu: cpu::Renderer,
    fonts: FastMap<FontId, Option<Font>>,
    /// Images by id, colour filters, ink and interpolation (hashed by [`filter_hash`]).
    images: FastMap<(ImageId, u64), Option<Embedded>>,
    /// Each structure element's marked content, and where it is first drawn (page, point).
    marks: FastMap<u32, Vec<Identifier>>,
    firsts: FastMap<u32, (usize, kurbo::Point)>,
    /// The page being written, in points.
    page: kurbo::Rect,
    /// Whether images drawn with linear sampling ask viewers to interpolate them.
    interpolate: bool,
    /// Scratch for rounded rects' paths.
    verbs: Vec<Verb>,
    points: Vec<Point>,
}

impl<'r> Writer<'r> {
    fn empty(res: &'r Resources) -> Writer<'r> {
        Writer {
            res,
            cpu: cpu::Renderer::new(),
            fonts: FastMap::default(),
            images: FastMap::default(),
            marks: FastMap::default(),
            firsts: FastMap::default(),
            page: kurbo::Rect::ZERO,
            interpolate: true,
            verbs: Vec::new(),
            points: Vec::new(),
        }
    }

    fn element(&mut self, e: &Element) -> Node {
        let kind: TagKind = match &e.kind {
            Kind::Paragraph => Tag::P.into(),
            Kind::Heading(level) => Tag::Hn(NonZeroU16::new(u16::from(*level).clamp(1, 6)).unwrap_or(NonZeroU16::MIN), None).into(),
            Kind::List => Tag::L(ListNumbering::None).into(),
            Kind::ListItem => Tag::LI.into(),
            Kind::ListBody => Tag::LBody.into(),
            Kind::Table => Tag::Table.into(),
            Kind::Row => Tag::TR.into(),
            Kind::HeaderCell => Tag::TH(TableHeaderScope::Column).into(),
            Kind::Cell => Tag::TD.into(),
            Kind::Figure(alt) => Tag::Figure(alt.clone()).into(),
        };
        let mut g = TagGroup::new(kind);
        for id in e.content.and_then(|c| self.marks.remove(&c)).unwrap_or_default() {
            g.push(id);
        }
        for k in &e.kids {
            g.push(self.element(k));
        }
        g.into()
    }

    /// The bookmarks: each heading under the last one of a lower level before it.
    fn outline(&self, headings: &[Heading]) -> Outline {
        let mut outline = Outline::new();
        let mut open: Vec<(u8, OutlineNode)> = Vec::new();
        let close = |open: &mut Vec<(u8, OutlineNode)>, outline: &mut Outline| {
            if let Some((_, n)) = open.pop() {
                match open.last_mut() {
                    Some((_, parent)) => parent.push_child(n),
                    None => outline.push_child(n),
                }
            }
        };
        for h in headings {
            let Some(&(page, at)) = self.firsts.get(&h.content) else { continue };
            while open.last().is_some_and(|(l, _)| *l >= h.level) {
                close(&mut open, &mut outline);
            }
            let dest = XyzDestination::new(page, KPoint::from_xy(at.x as f32, at.y as f32));
            open.push((h.level, OutlineNode::new(h.title.clone(), dest)));
        }
        while !open.is_empty() {
            close(&mut open, &mut outline);
        }
        outline
    }

    /// Draws `picture` under `ctm` (its space to the page's).
    fn picture(&mut self, s: &mut Surface<'_>, picture: &Picture, ctm: Affine, ink: Ink) {
        let lists = picture.lists();
        match classify(lists, 0, false) {
            Group::Plain => self.walk(s, lists, &mut 0, ctm, false, ink),
            Group::Masked => self.masked(s, lists, &mut 0, ctm, false, ink),
            Group::Raster => {
                let area = picture.bounds().map_or(self.page, |b| ctm.transform_rect_bbox(b).intersect(self.page));
                self.raster(s, area, ctm, ink, |cpu, target, res, t| {
                    cpu.render(target, res, &cpu::Frame { clear: None, draws: &[Draw { picture, transform: t }] })
                });
            }
        }
    }

    /// Writes `lists`' commands from `*i` as they are: to the end, or (`in_layer`) through the pop that ends the layer.
    fn walk(&mut self, s: &mut Surface<'_>, lists: &Lists, i: &mut usize, ctm: Affine, in_layer: bool, ink: Ink) {
        let mut ctm = ctm;
        let mut saved: Vec<Affine> = Vec::new();
        while let Some(&cmd) = lists.cmds.get(*i) {
            *i += 1;
            match cmd {
                Cmd::PushTransform { transform } => {
                    let t = affine(lists.transforms[transform as usize]);
                    s.push_transform(&ktransform(t));
                    saved.push(ctm);
                    ctm *= t;
                }
                Cmd::PushClip { clip } => {
                    push_clip(s, lists, lists.clips[clip as usize]);
                    saved.push(ctm);
                }
                Cmd::PushLayer { .. } => self.layer(s, lists, i, ctm, ink),
                Cmd::Pop => match saved.pop() {
                    Some(c) => {
                        s.pop();
                        ctm = c;
                    }
                    None if in_layer => return,
                    None => {}
                },
                Cmd::Picture { child, transform } => {
                    let t = affine(lists.transforms[transform as usize]);
                    s.push_transform(&ktransform(t));
                    let child = &lists.children[child as usize];
                    self.walk(s, child.lists(), &mut 0, ctm * t, false, ink);
                    s.pop();
                }
                _ => self.draw(s, lists, cmd, ink),
            }
        }
        for _ in saved {
            s.pop();
        }
    }

    /// Writes the layer whose push is at `lists.cmds[*i - 1]`, moving `*i` past its pop.
    fn layer(&mut self, s: &mut Surface<'_>, lists: &Lists, i: &mut usize, ctm: Affine, ink: Ink) {
        let at = *i - 1;
        let Some(&Cmd::PushLayer { layer }) = lists.cmds.get(at) else { return };
        let rec = lists.layers[layer as usize];
        let group = classify(lists, *i, true);
        let blend = native(rec.blend);
        let pushes = usize::from(blend.is_some()) + usize::from(rec.opacity < 1.0);
        if let Some(b) = blend {
            s.push_blend_mode(b);
        }
        if rec.opacity < 1.0 {
            s.push_opacity(NormalizedF32::new(rec.opacity.max(0.0)).unwrap_or(NormalizedF32::ZERO));
        }
        if rec.filters.len > 0 || rec.color_filters.len > 0 || group == Group::Raster {
            crate::cpu::skip_layer(lists, i);
            let reach: f64 = lists.filters[rec.filters.range()]
                .iter()
                .map(|f| match *f {
                    crate::scene::Filter::Blur { sigma } => 3.0 * f64::from(sigma.abs()),
                    crate::scene::Filter::Erode { radius } => f64::from(radius.abs()),
                })
                .sum();
            let local = rec.bounds.or(rec.content).map(Rect::to_kurbo);
            let area = local.map_or(self.page, |b| ctm.transform_rect_bbox(b.inflate(reach, reach)).intersect(self.page));
            self.raster(s, area, ctm, ink, |cpu, target, res, t| cpu.render_layer(target, res, lists, at, Transform::from_kurbo(t)));
        } else {
            s.push_isolated();
            let bounded = rec.bounds.map(|b| push_rect_clip(s, b.to_kurbo())).is_some();
            match group {
                Group::Masked => self.masked(s, lists, i, ctm, true, ink),
                _ => self.walk(s, lists, i, ctm, true, ink),
            }
            if bounded {
                s.pop();
            }
            s.pop();
        }
        for _ in 0..pushes {
            s.pop();
        }
    }

    /// Draws `area` (page points) of what `render` draws there, rendered at 300 dots per inch and embedded as an image.
    /// `render` draws under the transform from the drawing's space to the image's pixels.
    fn raster(
        &mut self,
        s: &mut Surface<'_>,
        area: kurbo::Rect,
        ctm: Affine,
        ink: Ink,
        render: impl FnOnce(&mut cpu::Renderer, &mut Target<'_>, &Resources, Affine),
    ) {
        let px = Affine::scale(RASTER).transform_rect_bbox(area).expand();
        let (w, h) = (px.width() as u32, px.height() as u32);
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 1 << 28 {
            return;
        }
        let mut pixels = vec![0u8; w as usize * h as usize * 4];
        if let Ok(mut target) = Target::new(&mut pixels, w, h, w as usize * 4, Format::Rgba8) {
            let t = Affine::translate((-px.x0, -px.y0)) * Affine::scale(RASTER) * ctm;
            render(&mut self.cpu, &mut target, self.res, t);
        }
        let image = rgba_image(pixels, w, h, ink, self.interpolate);
        // The image in page points, wherever the transform is now.
        let place = ctm.inverse() * Affine::translate((px.x0 / RASTER, px.y0 / RASTER));
        s.push_transform(&ktransform(place));
        if let Some(size) = Size::from_wh(w as f32 / RASTER as f32, h as f32 / RASTER as f32) {
            s.draw_image(image, size);
        }
        s.pop();
    }

    /// Writes a group (from `*i`, as [`Writer::walk`]) whose Porter-Duff draws and layers become soft masks.
    fn masked(&mut self, s: &mut Surface<'_>, lists: &Lists, i: &mut usize, ctm: Affine, in_layer: bool, ink: Ink) {
        let mut ops = Vec::new();
        collect(lists, i, in_layer, &mut Vec::new(), &mut ops);
        let mut acc: Vec<Piece> = Vec::new();
        for (k, op) in ops.iter().enumerate() {
            let piece = Piece::Op(k);
            acc = match (op.lists.cmds[op.at], op_mode(op)) {
                (_, BlendMode::Dst) => acc,
                (Cmd::PushLayer { layer }, BlendMode::DstIn) => {
                    let kind = op.lists.layers[layer as usize].bounds.map_or(MaskKind::Alpha, |b| MaskKind::AlphaInside(b.to_kurbo()));
                    vec![Piece::Masked { content: acc, mask: vec![piece], kind }]
                }
                (Cmd::PushLayer { .. }, BlendMode::SrcIn) => vec![Piece::Masked { content: vec![piece], mask: acc, kind: MaskKind::Alpha }],
                (_, BlendMode::Clear) => vec![Piece::Masked { content: acc, mask: vec![piece], kind: MaskKind::Inverse(Ink::Black) }],
                (_, BlendMode::DstOut) => vec![Piece::Masked { content: acc, mask: vec![piece], kind: MaskKind::Inverse(Ink::BlackAlpha) }],
                _ => {
                    acc.push(piece);
                    acc
                }
            };
        }
        self.pieces(s, &acc, &ops, ctm, ink);
    }

    fn pieces(&mut self, s: &mut Surface<'_>, pieces: &[Piece], ops: &[Op<'_>], ctm: Affine, ink: Ink) {
        for p in pieces {
            match p {
                Piece::Op(k) => self.op(s, &ops[*k], ctm, ink),
                Piece::Masked { content, mask, kind } => {
                    let mut sb = s.stream_builder();
                    {
                        let mut m = sb.surface();
                        // Everywhere the page shows, in this space.
                        let area = ctm.inverse().transform_rect_bbox(self.page);
                        match *kind {
                            MaskKind::Alpha => self.pieces(&mut m, mask, ops, ctm, ink),
                            MaskKind::AlphaInside(b) => {
                                let mut outside = PathBuilder::new();
                                rect_into(&mut outside, area);
                                rect_into(&mut outside, b);
                                if let Some(path) = outside.finish() {
                                    m.set_fill(Some(solid(rgb::Color::black(), 1.0, KRule::EvenOdd)));
                                    m.draw_path(&path);
                                }
                                push_rect_clip(&mut m, b);
                                self.pieces(&mut m, mask, ops, ctm, ink);
                                m.pop();
                            }
                            MaskKind::Inverse(black) => {
                                if let Some(path) = rect_path(area) {
                                    m.set_fill(Some(solid(rgb::Color::white(), 1.0, KRule::NonZero)));
                                    m.draw_path(&path);
                                }
                                self.pieces(&mut m, mask, ops, ctm, black);
                            }
                        }
                        m.finish();
                    }
                    let stream = sb.finish();
                    let mask_type = if matches!(kind, MaskKind::Inverse(_)) { MaskType::Luminosity } else { MaskType::Alpha };
                    s.push_mask(Mask::new(stream, mask_type));
                    self.pieces(s, content, ops, ctm, ink);
                    s.pop();
                }
            }
        }
    }

    /// A masked group's draw or layer, under its own transforms and clips; its Porter-Duff mode is what the masks did.
    fn op(&mut self, s: &mut Surface<'_>, op: &Op<'_>, ctm: Affine, ink: Ink) {
        let mut ctm = ctm;
        for c in &op.context {
            match *c {
                Context::Transform(t) => {
                    s.push_transform(&ktransform(t));
                    ctm *= t;
                }
                Context::Clip(lists, rec) => push_clip(s, lists, rec),
            }
        }
        match op.lists.cmds[op.at] {
            Cmd::PushLayer { .. } => self.layer(s, op.lists, &mut (op.at + 1), ctm, ink),
            cmd => self.draw(s, op.lists, cmd, ink),
        }
        for _ in &op.context {
            s.pop();
        }
    }

    /// Writes one draw command under `ctm`.
    fn draw(&mut self, s: &mut Surface<'_>, lists: &Lists, cmd: Cmd, ink: Ink) {
        let (path, paint, even_odd) = match cmd {
            Cmd::SolidRect { rect, color, blend, .. } => {
                if blend == BlendMode::Dst {
                    return;
                }
                let Some(path) = rect_path(rect.to_kurbo()) else { return };
                let fill = premul_fill(color, ink, KRule::NonZero);
                with_blend(s, blend, |s| {
                    s.set_fill(Some(fill));
                    s.set_stroke(None);
                    s.draw_path(&path);
                });
                return;
            }
            Cmd::Rect { rect, paint } => (rect_path(rect.to_kurbo()), paint, false),
            Cmd::RRect { rrect, paint } => {
                let (rect, radii) = lists.rrects[rrect as usize];
                let (mut verbs, mut points) = (std::mem::take(&mut self.verbs), std::mem::take(&mut self.points));
                verbs.clear();
                points.clear();
                rrect_path(rect, radii, &mut verbs, &mut points);
                let path = path_of(&verbs, &points);
                (self.verbs, self.points) = (verbs, points);
                (path, paint, false)
            }
            Cmd::Frame { frame, paint } => {
                let (outer, inner) = lists.frames[frame as usize];
                let (mut verbs, mut points) = (std::mem::take(&mut self.verbs), std::mem::take(&mut self.points));
                verbs.clear();
                points.clear();
                frame_path(outer, inner, &mut verbs, &mut points);
                let path = path_of(&verbs, &points);
                (self.verbs, self.points) = (verbs, points);
                (path, paint, false)
            }
            Cmd::Path { path, paint } | Cmd::EvenOddPath { path, paint } => {
                let even_odd = matches!(cmd, Cmd::EvenOddPath { .. });
                (path_of(&lists.verbs[path.verbs.range()], &lists.points[path.points.range()]), paint, even_odd)
            }
            Cmd::Glyphs { run, paint } => return self.glyphs(s, lists, run as usize, paint as usize, ink),
            Cmd::Image { image } => return self.image(s, lists, image as usize, ink),
            _ => return,
        };
        let Some(path) = path else { return };
        let rec = lists.paints[paint as usize];
        if rec.blend == BlendMode::Dst {
            return;
        }
        let rule = if even_odd { KRule::EvenOdd } else { KRule::NonZero };
        let Some(fill) = self.fill(s, lists, paint as usize, ink, rule) else { return };
        with_blend(s, rec.blend, |s| {
            s.set_fill(Some(fill));
            s.set_stroke(None);
            s.draw_path(&path);
        });
    }

    /// Paint `index` as a fill.
    fn fill(&mut self, s: &mut Surface<'_>, lists: &Lists, index: usize, ink: Ink, rule: KRule) -> Option<Fill> {
        let rec = lists.paints[index];
        if let BrushRec::Solid(c) = rec.brush {
            return Some(premul_fill(c, ink, rule));
        }
        if ink == Ink::Black {
            return Some(solid(rgb::Color::black(), 1.0, rule));
        }
        let stops = |span: crate::scene::Span| -> Vec<KStop> {
            lists.stops[span.range()]
                .iter()
                .map(|s| KStop {
                    offset: NormalizedF32::new(s.offset.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ZERO),
                    color: if ink == Ink::Paint { colour(s.color).into() } else { rgb::Color::black().into() },
                    opacity: NormalizedF32::new(f32::from(s.color.a) / 255.0).unwrap_or(NormalizedF32::ONE),
                })
                .collect()
        };
        let spread = |e: Extend| match e {
            Extend::Clamp => SpreadMethod::Pad,
            Extend::Repeat => SpreadMethod::Repeat,
            Extend::Mirror => SpreadMethod::Reflect,
        };
        let paint: KPaint = match rec.brush {
            BrushRec::Solid(_) => unreachable!("handled above"),
            BrushRec::Linear { start, end, stops: st, extend, transform } => LinearGradient {
                x1: start.x,
                y1: start.y,
                x2: end.x,
                y2: end.y,
                transform: ktransform(affine(transform)),
                spread_method: spread(extend),
                stops: stops(st),
                anti_alias: false,
            }
            .into(),
            BrushRec::Radial { start, start_radius, end, end_radius, stops: st, extend, transform } => RadialGradient {
                fx: start.x,
                fy: start.y,
                fr: start_radius,
                cx: end.x,
                cy: end.y,
                cr: end_radius,
                transform: ktransform(affine(transform)),
                spread_method: spread(extend),
                stops: stops(st),
                anti_alias: false,
            }
            .into(),
            BrushRec::Sweep { center, start_angle, end_angle, stops: st, extend, transform } => SweepGradient {
                cx: center.x,
                cy: center.y,
                start_angle,
                end_angle,
                transform: ktransform(affine(transform)),
                spread_method: spread(extend),
                stops: stops(st),
                anti_alias: false,
            }
            .into(),
            BrushRec::Pattern { image, transform, sampling } => {
                let e = self.krilla_image(image, &[], ink, sampling)?;
                let mut sb = s.stream_builder();
                {
                    let mut p = sb.surface();
                    p.push_transform(&ktransform(e.orient));
                    if let Some(size) = Size::from_wh(e.stored.0 as f32, e.stored.1 as f32) {
                        p.draw_image(e.image, size);
                    }
                    p.pop();
                    p.finish();
                }
                let (uw, uh) = e.upright;
                Pattern { stream: sb.finish(), transform: ktransform(affine(transform)), width: uw as f32, height: uh as f32 }.into()
            }
        };
        Some(Fill { paint, opacity: NormalizedF32::ONE, rule })
    }

    fn glyphs(&mut self, s: &mut Surface<'_>, lists: &Lists, run: usize, paint: usize, ink: Ink) {
        let run = lists.runs[run];
        let Some(font) = self.font(run.font) else { return };
        let glyphs = &lists.glyphs[run.glyphs.range()];
        let (Some(first), size) = (glyphs.first(), run.size) else { return };
        let text = std::str::from_utf8(&lists.text.as_bytes()[run.text.range()]).unwrap_or("");
        let clusters = &lists.clusters[run.clusters.range()];
        let end = |c: u32| clusters.iter().copied().filter(|&d| d > c).min().map_or(text.len(), |d| d as usize);
        let kglyphs: Vec<KrillaGlyph> = glyphs
            .iter()
            .enumerate()
            .map(|(k, g)| {
                let advance = glyphs.get(k + 1).map_or(0.0, |n| (n.x - g.x) / size);
                let range = match clusters.get(k) {
                    Some(&c) => c as usize..end(c),
                    None if k == 0 => 0..text.len(),
                    None => text.len()..text.len(),
                };
                KrillaGlyph::new(GlyphId::new(g.id), advance, 0.0, 0.0, 0.0, range, None)
            })
            .collect();
        let rule = KRule::NonZero;
        let Some(fill) = self.fill(s, lists, paint, ink, rule) else { return };
        let rec = lists.paints[paint];
        if rec.blend == BlendMode::Dst {
            return;
        }
        let stroke = run.synthesis.embolden.then(|| KStroke {
            paint: fill.paint.clone(),
            width: size * embolden_em(size),
            opacity: fill.opacity,
            line_join: LineJoin::Miter,
            line_cap: LineCap::Butt,
            ..KStroke::default()
        });
        let skew = run.synthesis.skew_x;
        with_blend(s, rec.blend, |s| {
            if skew != 0.0 {
                let b = f64::from(run.baseline);
                s.push_transform(&ktransform(Affine::new([1.0, 0.0, f64::from(skew), 1.0, -f64::from(skew) * b, 0.0])));
            }
            s.set_fill(Some(fill));
            s.set_stroke(stroke);
            s.draw_glyphs(KPoint::from_xy(first.x, run.baseline), &kglyphs, font, text, size, false);
            s.set_stroke(None);
            if skew != 0.0 {
                s.pop();
            }
        });
    }

    fn font(&mut self, id: FontId) -> Option<Font> {
        let res = self.res;
        self.fonts
            .entry(id)
            .or_insert_with(|| {
                let (data, index, variations) = res.fonts.source(id)?;
                let data: krilla::Data = Arc::clone(data).into();
                if variations.is_empty() {
                    Font::new(data, index)
                } else {
                    let coords: Vec<(krilla::text::Tag, f32)> = variations.iter().map(|&(t, v)| (krilla::text::Tag::new(&t), v)).collect();
                    Font::new_variable(data, index, &coords)
                }
            })
            .clone()
    }

    fn image(&mut self, s: &mut Surface<'_>, lists: &Lists, index: usize, ink: Ink) {
        let rec = lists.images[index];
        let filters = &lists.color_filters[rec.color_filters.range()];
        let Some(e) = self.krilla_image(rec.image, filters, ink, rec.sampling) else { return };
        // Upright pixels to the draw's space: the source rect onto the destination.
        let (uw, uh) = e.upright;
        let src = rec.src.map_or(kurbo::Rect::new(0.0, 0.0, f64::from(uw), f64::from(uh)), Rect::to_kurbo);
        let dst = rec.dst.to_kurbo();
        if src.width() == 0.0 || src.height() == 0.0 {
            return;
        }
        let place = Affine::translate((dst.x0, dst.y0))
            * Affine::scale_non_uniform(dst.width() / src.width(), dst.height() / src.height())
            * Affine::translate((-src.x0, -src.y0))
            * e.orient;
        let clipped = rec.src.is_some();
        if clipped {
            push_rect_clip(s, dst.abs());
        }
        s.push_transform(&ktransform(place));
        if let Some(size) = Size::from_wh(e.stored.0 as f32, e.stored.1 as f32) {
            s.draw_image(e.image, size);
        }
        s.pop();
        if clipped {
            s.pop();
        }
    }

    /// Image `id` with `filters` applied, for PDF: an unfiltered JPEG as its file, anything else decoded. Its stored
    /// size, and the transform from stored pixels to upright ones (the JPEG's EXIF orientation; decoded images are
    /// upright already).
    fn krilla_image(&mut self, id: ImageId, filters: &[ColorFilter], ink: Ink, sampling: Sampling) -> Option<Embedded> {
        let interpolate = self.interpolate && sampling != Sampling::Nearest;
        let key = (id, filter_hash(filters, ink, interpolate));
        if let Some(found) = self.images.get(&key) {
            return found.clone();
        }
        let res = self.res;
        let made = (|| {
            if filters.is_empty()
                && ink == Ink::Paint
                && let Some((data, info)) = res.images.encoded(id)
                && info.format == ImageFormat::Jpeg
            {
                let stored = if info.orientation >= 5 { (info.height, info.width) } else { (info.width, info.height) };
                if let Ok(image) = Image::from_jpeg(Arc::clone(data).into(), interpolate) {
                    let orient = orientation(info.orientation, f64::from(stored.0), f64::from(stored.1));
                    return Some(Embedded { image, stored, upright: (info.width, info.height), orient });
                }
            }
            let info = res.images.info(id)?;
            let (w, h) = (info.width, info.height);
            let mut pixels = vec![0u8; w as usize * h as usize * 4];
            {
                // The image drawn at its size, through its colour filters, by the CPU backend.
                let mut rec = Recorder::new();
                rec.image(
                    id,
                    None,
                    kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
                    &ImagePaint { sampling: Sampling::Nearest, color_filters: filters },
                );
                let picture = rec.finish();
                let mut target = Target::new(&mut pixels, w, h, w as usize * 4, Format::Rgba8).ok()?;
                let draws = [Draw { picture: &picture, transform: Affine::IDENTITY }];
                self.cpu.render(&mut target, res, &cpu::Frame { clear: None, draws: &draws });
            }
            Some(Embedded { image: rgba_image(pixels, w, h, ink, interpolate), stored: (w, h), upright: (w, h), orient: Affine::IDENTITY })
        })();
        self.images.insert(key, made.clone());
        made
    }
}

/// How a group of commands (from `i`: to the end, or through the layer's pop) is written: the strongest need of its
/// draws and the layers directly in it, its nested pictures' commands included.
fn classify(lists: &Lists, mut i: usize, in_layer: bool) -> Group {
    let mut group = Group::Plain;
    let mut depth = 0usize;
    // Clips of the group's own, open: a masking mode under one acts only inside it, which the masks don't cover.
    let mut clips: Vec<bool> = Vec::new();
    while let Some(&cmd) = lists.cmds.get(i) {
        i += 1;
        let need = match cmd {
            Cmd::PushTransform { .. } => {
                depth += 1;
                clips.push(false);
                Group::Plain
            }
            Cmd::PushClip { .. } => {
                depth += 1;
                clips.push(true);
                Group::Plain
            }
            Cmd::Pop if depth == 0 => {
                if in_layer {
                    break;
                }
                Group::Plain
            }
            Cmd::Pop => {
                depth -= 1;
                clips.pop();
                Group::Plain
            }
            Cmd::Picture { child, .. } => classify(lists.children[child as usize].lists(), 0, false),
            Cmd::PushLayer { layer } => {
                let rec = lists.layers[layer as usize];
                crate::cpu::skip_layer(lists, &mut i);
                match rec.blend {
                    m if native(m).is_some() || m == BlendMode::SrcOver || m == BlendMode::Dst => Group::Plain,
                    BlendMode::DstIn if !clips.contains(&true) => Group::Masked,
                    BlendMode::SrcIn if !clips.contains(&true) && rec.bounds.is_none() => Group::Masked,
                    _ => Group::Raster,
                }
            }
            Cmd::Glyphs { paint, .. }
            | Cmd::Rect { paint, .. }
            | Cmd::RRect { paint, .. }
            | Cmd::Path { paint, .. }
            | Cmd::EvenOddPath { paint, .. }
            | Cmd::Frame { paint, .. } => {
                let rec = lists.paints[paint as usize];
                draw_need(rec.blend, matches!(rec.brush, BrushRec::Solid(_)))
            }
            Cmd::SolidRect { blend, .. } => draw_need(blend, true),
            Cmd::Image { .. } => Group::Plain,
        };
        group = group.max(need);
    }
    group
}

/// What a draw composited by `mode` needs: a mask for clearing and (solid) punching out, a raster for the other
/// Porter-Duff modes.
fn draw_need(mode: BlendMode, solid: bool) -> Group {
    match mode {
        m if native(m).is_some() || m == BlendMode::SrcOver || m == BlendMode::Dst => Group::Plain,
        BlendMode::Clear => Group::Masked,
        BlendMode::DstOut if solid => Group::Masked,
        _ => Group::Raster,
    }
}

/// The mode a masked group's op is composited by.
fn op_mode(op: &Op<'_>) -> BlendMode {
    match op.lists.cmds[op.at] {
        Cmd::PushLayer { layer } => op.lists.layers[layer as usize].blend,
        Cmd::SolidRect { blend, .. } => blend,
        Cmd::Glyphs { paint, .. } | Cmd::Rect { paint, .. } | Cmd::RRect { paint, .. } | Cmd::Path { paint, .. } | Cmd::EvenOddPath { paint, .. } | Cmd::Frame { paint, .. } => {
            op.lists.paints[paint as usize].blend
        }
        _ => BlendMode::SrcOver,
    }
}

/// A masked group's draws and layers (from `*i`, as [`Writer::walk`] reads them), each with the transforms and clips
/// of the group's own it is under; nested pictures' commands among them.
fn collect<'p>(lists: &'p Lists, i: &mut usize, in_layer: bool, context: &mut Vec<Context<'p>>, out: &mut Vec<Op<'p>>) {
    let base = context.len();
    while let Some(&cmd) = lists.cmds.get(*i) {
        *i += 1;
        match cmd {
            Cmd::PushTransform { transform } => context.push(Context::Transform(affine(lists.transforms[transform as usize]))),
            Cmd::PushClip { clip } => context.push(Context::Clip(lists, lists.clips[clip as usize])),
            Cmd::Pop if context.len() > base => {
                context.pop();
            }
            Cmd::Pop => {
                if in_layer {
                    return;
                }
            }
            Cmd::Picture { child, transform } => {
                context.push(Context::Transform(affine(lists.transforms[transform as usize])));
                collect(lists.children[child as usize].lists(), &mut 0, false, context, out);
                context.pop();
            }
            Cmd::PushLayer { .. } => {
                out.push(Op { lists, at: *i - 1, context: context.clone() });
                crate::cpu::skip_layer(lists, i);
            }
            _ => out.push(Op { lists, at: *i - 1, context: context.clone() }),
        }
    }
    context.truncate(base);
}

/// The PDF blend mode for `mode`, when PDF has it and it isn't source-over.
fn native(mode: BlendMode) -> Option<krilla::blend::BlendMode> {
    use krilla::blend::BlendMode as K;
    Some(match mode {
        BlendMode::Multiply => K::Multiply,
        BlendMode::Screen => K::Screen,
        BlendMode::Overlay => K::Overlay,
        BlendMode::Darken => K::Darken,
        BlendMode::Lighten => K::Lighten,
        BlendMode::ColorDodge => K::ColorDodge,
        BlendMode::ColorBurn => K::ColorBurn,
        BlendMode::HardLight => K::HardLight,
        BlendMode::SoftLight => K::SoftLight,
        BlendMode::Difference => K::Difference,
        BlendMode::Exclusion => K::Exclusion,
        BlendMode::Hue => K::Hue,
        BlendMode::Saturation => K::Saturation,
        BlendMode::Color => K::Color,
        BlendMode::Luminosity => K::Luminosity,
        _ => return None,
    })
}

/// Runs `draw` composited by `mode` (source-over and the Porter-Duff modes draw as source-over here; a masked group
/// did the rest).
fn with_blend(s: &mut Surface<'_>, mode: BlendMode, draw: impl FnOnce(&mut Surface<'_>)) {
    match native(mode) {
        Some(b) => {
            s.push_blend_mode(b);
            draw(s);
            s.pop();
        }
        None => draw(s),
    }
}

fn affine(t: Transform) -> Affine {
    Affine::new([t.a, t.b, t.c, t.d, t.e, t.f].map(f64::from))
}

fn ktransform(t: Affine) -> KTransform {
    let [a, b, c, d, e, f] = t.as_coeffs().map(|v| v as f32);
    KTransform::from_row(a, b, c, d, e, f)
}

fn colour(c: Color) -> rgb::Color {
    rgb::Color::new(c.r, c.g, c.b)
}

fn solid(c: rgb::Color, alpha: f32, rule: KRule) -> Fill {
    Fill { paint: c.into(), opacity: NormalizedF32::new(alpha.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE), rule }
}

/// A premultiplied colour as a fill, or black (opaque, or at its alpha) for a mask.
fn premul_fill(c: PremulColor, ink: Ink, rule: KRule) -> Fill {
    let alpha = f32::from(c.a) / 255.0;
    match ink {
        Ink::Black => solid(rgb::Color::black(), 1.0, rule),
        Ink::BlackAlpha => solid(rgb::Color::black(), alpha, rule),
        Ink::Paint => {
            let un = |v: u8| if c.a == 0 { 0 } else { ((u32::from(v) * 255 + u32::from(c.a) / 2) / u32::from(c.a)).min(255) as u8 };
            solid(rgb::Color::new(un(c.r), un(c.g), un(c.b)), alpha, rule)
        }
    }
}

fn path_of(verbs: &[Verb], points: &[Point]) -> Option<Path> {
    let mut b = PathBuilder::new();
    let mut p = points.iter();
    for v in verbs {
        match v {
            Verb::Move => {
                let a = p.next()?;
                b.move_to(a.x, a.y);
            }
            Verb::Line => {
                let a = p.next()?;
                b.line_to(a.x, a.y);
            }
            Verb::Quad => {
                let (c, a) = (p.next()?, p.next()?);
                b.quad_to(c.x, c.y, a.x, a.y);
            }
            Verb::Cubic => {
                let (c0, c1, a) = (p.next()?, p.next()?, p.next()?);
                b.cubic_to(c0.x, c0.y, c1.x, c1.y, a.x, a.y);
            }
            Verb::Close => b.close(),
        }
    }
    b.finish()
}

fn rect_into(b: &mut PathBuilder, r: kurbo::Rect) {
    let r = r.abs();
    b.move_to(r.x0 as f32, r.y0 as f32);
    b.line_to(r.x1 as f32, r.y0 as f32);
    b.line_to(r.x1 as f32, r.y1 as f32);
    b.line_to(r.x0 as f32, r.y1 as f32);
    b.close();
}

fn rect_path(r: kurbo::Rect) -> Option<Path> {
    let mut b = PathBuilder::new();
    rect_into(&mut b, r);
    b.finish()
}

/// Pushes a clip to `r`; the caller pops it. A rect that makes no path clips everything.
fn push_rect_clip(s: &mut Surface<'_>, r: kurbo::Rect) {
    let path = rect_path(r).or_else(|| rect_path(kurbo::Rect::new(0.0, 0.0, 0.0, 0.0)));
    match path {
        Some(p) => s.push_clip_path(&p, &KRule::NonZero),
        None => {
            // An empty clip: a tiny rect away from anything drawn.
            let mut b = PathBuilder::new();
            rect_into(&mut b, kurbo::Rect::new(-2.0, -2.0, -1.0, -1.0));
            let p = b.finish().expect("a unit square is a path");
            s.push_clip_path(&p, &KRule::NonZero);
        }
    }
}

fn push_clip(s: &mut Surface<'_>, lists: &Lists, rec: ClipRec) {
    match rec {
        ClipRec::Rect { rect, .. } => push_rect_clip(s, rect.to_kurbo()),
        ClipRec::Path { path, even_odd } => {
            let rule = if even_odd { KRule::EvenOdd } else { KRule::NonZero };
            match path_of(&lists.verbs[path.verbs.range()], &lists.points[path.points.range()]) {
                Some(p) => s.push_clip_path(&p, &rule),
                // An empty path keeps nothing.
                None => push_rect_clip(s, kurbo::Rect::new(-2.0, -2.0, -1.0, -1.0)),
            }
        }
    }
}

/// The transform from a JPEG's stored pixels (`w × h`) to its upright view at EXIF orientation `o`: the inverse of
/// the sampling `image::orient` does.
fn orientation(o: u8, w: f64, h: f64) -> Affine {
    Affine::new(match o {
        2 => [-1.0, 0.0, 0.0, 1.0, w, 0.0],
        3 => [-1.0, 0.0, 0.0, -1.0, w, h],
        4 => [1.0, 0.0, 0.0, -1.0, 0.0, h],
        5 => [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
        6 => [0.0, 1.0, -1.0, 0.0, h, 0.0],
        7 => [0.0, -1.0, -1.0, 0.0, h, w],
        8 => [0.0, -1.0, 1.0, 0.0, 0.0, w],
        _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    })
}

/// A hash of colour filters, the ink and the interpolation; 0 for none, the paint and interpolated.
fn filter_hash(filters: &[ColorFilter], ink: Ink, interpolate: bool) -> u64 {
    if filters.is_empty() && ink == Ink::Paint && interpolate {
        return 0;
    }
    let mut h = std::hash::DefaultHasher::new();
    (ink as u8, interpolate).hash(&mut h);
    for f in filters {
        match f {
            ColorFilter::Matrix(m) => m.iter().for_each(|v| v.to_bits().hash(&mut h)),
            ColorFilter::Table(t) => t.hash(&mut h),
        }
    }
    h.finish() | 1
}

/// Premultiplied RGBA pixels as an image: unpremultiplied colour (black for a mask's ink) and, when any pixel isn't
/// opaque, alpha.
fn rgba_image(pixels: Vec<u8>, w: u32, h: u32, ink: Ink, interpolate: bool) -> Image {
    let mut color = Vec::with_capacity(w as usize * h as usize * 3);
    let mut alpha = Vec::with_capacity(w as usize * h as usize);
    for p in pixels.as_chunks::<4>().0 {
        let a = p[3];
        let un = |v: u8| if a == 0 { 0 } else { ((u32::from(v) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8 };
        if ink == Ink::Paint {
            color.extend([un(p[0]), un(p[1]), un(p[2])]);
        } else {
            color.extend([0, 0, 0]);
        }
        alpha.push(a);
    }
    let alpha = alpha.iter().any(|&a| a != 255).then(|| Arc::new(alpha));
    let image = Pixels { size: (w, h), color: Arc::new(color), alpha };
    Image::from_custom(image, interpolate).expect("the channels match the size")
}

/// Decoded pixels for krilla: 8-bit RGB, and alpha when not opaque.
#[derive(Clone, Debug)]
struct Pixels {
    size: (u32, u32),
    color: Arc<Vec<u8>>,
    alpha: Option<Arc<Vec<u8>>>,
}

impl Hash for Pixels {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.size.hash(state);
        self.color.hash(state);
        self.alpha.hash(state);
    }
}

impl CustomImage for Pixels {
    fn color_channel(&self) -> &[u8] {
        &self.color
    }

    fn alpha_channel(&self) -> Option<&[u8]> {
        self.alpha.as_deref().map(Vec::as_slice)
    }

    fn bits_per_component(&self) -> BitsPerComponent {
        BitsPerComponent::Eight
    }

    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn icc_profile(&self) -> Option<&[u8]> {
        None
    }

    fn color_space(&self) -> ImageColorspace {
        ImageColorspace::Rgb
    }
}
