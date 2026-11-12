//! PDF: pictures written as the pages of a PDF with typst's `pdf-writer`. Each page is a list of pictures, each marked
//! as a structure element's content or as an artifact; the structure tree, the outline (from the headings) and the
//! metadata come with them. The output is PDF 1.7 with a classic cross-reference table, claiming PDF/A-2b when asked
//! (XMP metadata, an sRGB output intent, every font embedded, no image interpolation).
//!
//! Text: a TrueType face (glyf outlines) at its default instance is embedded subset, as a CID font with Identity
//! encoding (glyph ids kept) and a ToUnicode map from the runs' characters. Other glyphs (CFF and CFF2 outlines,
//! faces at variation coordinates, COLR and bitmap colour glyphs) are Type 3 glyphs: their outlines at the face's
//! coordinates, or the colour glyph drawn by the CPU backend as an image. A run whose characters don't come back from
//! its glyphs' ToUnicode entries (ligatures, reordered clusters) carries them as ActualText. Emboldened text is filled
//! and stroked (Skia's fake bold); emboldened Type 3 text and text painted with a gradient are drawn as outlines with
//! their characters as ActualText.
//!
//! What PDF has no operator for is composited as Skia's PDF backend does, with soft masks: a fill that clears
//! (`Clear`) or punches out (`DstOut`, solid paint) masks what its layer drew before it by the fill's inverse; a layer
//! composited destination-in masks what came before it by its alpha, and one composited source-in is drawn masked by
//! the alpha of what came before (which it replaces). Layers with filters or colour filters, and groups composited in
//! ways the masks don't cover (other Porter-Duff modes, or those modes under a clip of the group's own), are drawn by
//! the CPU backend at 300 dots per inch and embedded as images, as Skia rasterises what PDF can't express.
//!
//! Gradients are shadings, drawn inside the shape's clip: axial and radial ones with stitched exponential functions
//! (repeated and mirrored ones unrolled over the shape), sweeps as function-based shadings with a PostScript
//! function. Stops with alpha add a soft mask of the same gradient's alpha.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};

use kurbo::{Affine, BezPath, PathEl, Shape};
use pdf_writer::types::{
    ArtifactType, BlendMode as PdfBlend, CidFontType, FontFlags, FunctionShadingType, MaskType, OutputIntentSubtype, PaintType, StructRole,
    SystemInfo, TilingType, UnicodeCmap,
};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect as PdfRect, Ref, Str, TextStr};
use skrifa::MetadataProvider;
use skrifa::raw::TableProvider;

use crate::Resources;
use crate::cover::{frame_path, rrect_path};
use crate::cpu::{self, Draw, Format, Target};
use crate::font::{FontId, GlyphKind, embolden_em};
use crate::fx::FastMap;
use crate::geom::{Point, Rect, Transform};
use crate::image::{ImageFormat, ImageId};
use crate::paint::{BlendMode, Color, ColorFilter, Extend, ImagePaint, PremulColor, Sampling, Stop};
use crate::path::Verb;
use crate::scene::{BrushRec, ClipRec, Cmd, GlyphRun, Lists, Picture, Recorder, RunRec, Synthesis};

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

/// A structure element: what it is, the content marked with its id (before its kids), and its kids.
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

/// A written PDF, and whether it claims PDF/A-2b.
#[derive(Clone, Debug)]
pub struct Written {
    pub bytes: Vec<u8>,
    pub archival: bool,
}

/// Why the PDF couldn't be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the PDF couldn't be written: {}", self.0)
    }
}

impl std::error::Error for Error {}

/// Device pixels per point of what is rasterised: 300 dots per inch.
const RASTER: f64 = 300.0 / 72.0;

/// Pixels per em colour glyphs are drawn at for their Type 3 images.
// shortcut: COLR glyphs are images, not vector (their layers as paths, gradients as shadings, as Skia writes them);
// about 18 KB per glyph. Vector COLR lands with the later PDF work (ROADMAP, Phase 9).
const COLOUR_GLYPH_PX: f64 = 256.0;

/// Writes `pages` as a PDF, tagged with `structure` (the tree's top level, in reading order), with bookmarks from
/// `outline` and `info`'s metadata.
pub fn write(res: &Resources, pages: &[Page], structure: &[Element], outline: &[Heading], info: &Info) -> Result<Written, Error> {
    let mut w = Writer::new(res, info.archival);
    let catalog = w.alloc();
    let tree = w.alloc();
    let page_refs: Vec<Ref> = pages.iter().map(|_| w.alloc()).collect();
    for (index, page) in pages.iter().enumerate() {
        w.page(index, page, page_refs[index], tree);
    }
    w.pdf.pages(tree).kids(page_refs.iter().copied()).count(pages.len() as i32);
    w.fonts_out();
    let struct_root = w.structure(structure, &page_refs);
    let outline_root = w.outline(outline, pages, &page_refs);
    w.document(catalog, tree, struct_root, outline_root, info);
    Ok(Written { bytes: w.pdf.finish(), archival: info.archival })
}

/// `data` (a single-face TrueType file, or face `index` of a collection) with the face's glyph ids but only the `used`
/// glyphs' outlines (and their components), without shaping and hinting tables: what a PDF embeds. `None` for a CFF
/// face, a file without the tables a PDF needs, or one that doesn't read.
pub fn subset_truetype(data: &[u8], index: u32, used: &std::collections::BTreeSet<u16>) -> Option<Vec<u8>> {
    subset_glyf(data, index, used, true)
}

/// [`subset_truetype`], keeping the character map and names (`whole`) or not: a PDF addresses an embedded face's glyphs
/// by id, and names it itself.
fn subset_glyf(data: &[u8], index: u32, used: &std::collections::BTreeSet<u16>, whole: bool) -> Option<Vec<u8>> {
    let be16 = |b: &[u8], o: usize| b.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let be32 = |b: &[u8], o: usize| b.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let base = if data.get(..4)? == b"ttcf" { be32(data, 12 + 4 * index as usize)? as usize } else { 0 };
    let mut tables: BTreeMap<[u8; 4], &[u8]> = BTreeMap::new();
    for i in 0..be16(data, base + 4)? as usize {
        let r = base + 12 + 16 * i;
        let (off, len) = (be32(data, r + 8)? as usize, be32(data, r + 12)? as usize);
        tables.insert(data.get(r..r + 4)?.try_into().ok()?, data.get(off..off + len)?);
    }
    let (head, maxp, loca, glyf) = (*tables.get(b"head")?, *tables.get(b"maxp")?, *tables.get(b"loca")?, *tables.get(b"glyf")?);
    let long = be16(head, 50)? != 0;
    let count = be16(maxp, 4)? as usize;
    let at = |g: usize| if long { be32(loca, 4 * g).map(|o| o as usize) } else { be16(loca, 2 * g).map(|o| 2 * o as usize) };
    // Glyph data that doesn't read is kept as no outline.
    let outline = |g: usize| at(g).zip(at(g + 1)).and_then(|(a, b)| glyf.get(a..b)).unwrap_or(&[]);

    // Composite glyphs draw their components: keep those too.
    let mut keep: std::collections::BTreeSet<u16> = used.iter().copied().filter(|&g| (g as usize) < count).collect();
    keep.insert(0);
    let mut todo: Vec<u16> = keep.iter().copied().collect();
    while let Some(g) = todo.pop() {
        let o = outline(g as usize);
        if o.len() < 10 || be16(o, 0)? as i16 >= 0 {
            continue;
        }
        let mut p = 10;
        while let (Some(flags), Some(component)) = (be16(o, p), be16(o, p + 2)) {
            if (component as usize) < count && keep.insert(component) {
                todo.push(component);
            }
            p += 4 + if flags & 1 != 0 { 4 } else { 2 };
            p += if flags & 0x80 != 0 {
                8
            } else if flags & 0x40 != 0 {
                4
            } else if flags & 0x08 != 0 {
                2
            } else {
                0
            };
            if flags & 0x20 == 0 {
                break;
            }
        }
    }
    let (mut new_glyf, mut new_loca) = (Vec::new(), Vec::with_capacity(4 * (count + 1)));
    for g in 0..count {
        new_loca.extend((new_glyf.len() as u32).to_be_bytes());
        if keep.contains(&(g as u16)) {
            new_glyf.extend_from_slice(outline(g));
            new_glyf.resize(new_glyf.len().next_multiple_of(4), 0);
        }
    }
    new_loca.extend((new_glyf.len() as u32).to_be_bytes());
    let mut new_head = head.to_vec();
    new_head.get_mut(8..12)?.fill(0); // checksumAdjustment
    new_head.get_mut(50..52)?.copy_from_slice(&1u16.to_be_bytes()); // long loca
    // post format 3: no glyph names.
    let post = tables.get(b"post").filter(|p| p.len() >= 32).map(|p| [&0x0003_0000u32.to_be_bytes()[..], &p[4..32]].concat());

    let mut out_tables: Vec<([u8; 4], Vec<u8>)> = vec![(*b"glyf", new_glyf), (*b"head", new_head), (*b"loca", new_loca)];
    out_tables.extend(post.map(|p| (*b"post", p)));
    for tag in [b"hhea", b"hmtx", b"maxp", b"OS/2"] {
        out_tables.push((*tag, tables.get(tag)?.to_vec()));
    }
    if whole {
        for tag in [b"cmap", b"name"] {
            out_tables.push((*tag, tables.get(tag)?.to_vec()));
        }
    }
    out_tables.sort_by_key(|t| t.0);
    let n = out_tables.len();
    let selector = n.ilog2();
    let mut out = Vec::new();
    out.extend(0x0001_0000u32.to_be_bytes());
    for v in [n as u16, 16 << selector, selector as u16, (n as u16) * 16 - (16 << selector)] {
        out.extend(v.to_be_bytes());
    }
    let mut offset = 12 + 16 * n;
    for (tag, body) in &out_tables {
        let sum = body.chunks(4).fold(0u32, |s, c| {
            let mut w = [0u8; 4];
            w[..c.len()].copy_from_slice(c);
            s.wrapping_add(u32::from_be_bytes(w))
        });
        out.extend(tag);
        out.extend(sum.to_be_bytes());
        out.extend((offset as u32).to_be_bytes());
        out.extend((body.len() as u32).to_be_bytes());
        offset += body.len().next_multiple_of(4);
    }
    for (_, body) in &out_tables {
        out.extend(body);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    Some(out)
}

/// How a draw is painted: with its own paint, or (in a mask made from it) black, opaque or at its paint's alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// A content stream being written (the page's, or a form's), the resources it names, and the transform from its own
/// space to the page's (y down from the top left).
struct Canvas {
    content: Content,
    fonts: BTreeMap<String, Ref>,
    xobjects: BTreeMap<String, Ref>,
    states: BTreeMap<String, Ref>,
    shadings: BTreeMap<String, Ref>,
    patterns: BTreeMap<String, Ref>,
    base: Affine,
}

impl Canvas {
    fn new(base: Affine) -> Canvas {
        Canvas {
            content: Content::new(),
            fonts: BTreeMap::new(),
            xobjects: BTreeMap::new(),
            states: BTreeMap::new(),
            shadings: BTreeMap::new(),
            patterns: BTreeMap::new(),
            base,
        }
    }
}

/// An embedded TrueType face's glyphs: their widths (1000 units per em) and characters.
#[derive(Default)]
struct CidFont {
    name: String,
    object: Option<Ref>,
    glyphs: BTreeMap<u16, (f32, String)>,
}

/// A Type 3 font: up to 256 glyphs of a face (and, for colour glyphs, a colour and ink), each a code.
struct Type3 {
    name: String,
    object: Ref,
    face: FontId,
    /// The colour glyphs are drawn in (their foreground), when they are colour glyphs.
    colour: Option<(Color, Ink)>,
    glyphs: Vec<(u32, String)>,
}

/// What a Type 3 font holds: a face's glyphs, outlines or (with a colour and ink) colour glyphs.
type Type3Key = (FontId, Option<(Color, Ink)>);

/// An image as embedded: its XObject, stored size, upright size, and the transform from stored pixels to upright ones.
#[derive(Clone, Copy)]
struct Embedded {
    object: Ref,
    stored: (u32, u32),
    upright: (u32, u32),
    orient: Affine,
}

/// Where a face's glyphs go.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FaceKind {
    /// A TrueType face at its default instance: embedded, subset.
    Cid,
    /// Type 3 outlines.
    Outlines,
}

struct Writer<'r> {
    res: &'r Resources,
    pdf: Pdf,
    next: Ref,
    cpu: cpu::Renderer,
    canvases: Vec<Canvas>,
    archival: bool,
    /// The page being written, in points, and its index.
    page: kurbo::Rect,
    page_index: usize,
    /// Each page's marked content: (marked-content id, structure content id).
    marks: Vec<Vec<(i32, u32)>>,
    /// Where each structure element's content is first drawn: page, point (y down).
    firsts: FastMap<u32, (usize, kurbo::Point)>,
    faces: FastMap<FontId, Option<FaceKind>>,
    cid: FastMap<FontId, CidFont>,
    type3: Vec<Type3>,
    /// Type 3 fonts by face and colour, the last one still taking glyphs; a glyph's font and code.
    type3_open: FastMap<Type3Key, usize>,
    type3_codes: FastMap<(Type3Key, u32), (usize, u8)>,
    images: FastMap<(ImageId, u64), Option<Embedded>>,
    states: FastMap<String, (String, Ref)>,
    names: u32,
    /// Scratch for paths.
    verbs: Vec<Verb>,
    points: Vec<Point>,
}

impl<'r> Writer<'r> {
    fn new(res: &'r Resources, archival: bool) -> Writer<'r> {
        Writer {
            res,
            pdf: Pdf::with_settings(pdf_writer::Settings { pretty: false }),
            next: Ref::new(1),
            cpu: cpu::Renderer::new(),
            canvases: Vec::new(),
            archival,
            page: kurbo::Rect::ZERO,
            page_index: 0,
            marks: Vec::new(),
            firsts: FastMap::default(),
            faces: FastMap::default(),
            cid: FastMap::default(),
            type3: Vec::new(),
            type3_open: FastMap::default(),
            type3_codes: FastMap::default(),
            images: FastMap::default(),
            states: FastMap::default(),
            names: 0,
            verbs: Vec::new(),
            points: Vec::new(),
        }
    }

    fn alloc(&mut self) -> Ref {
        self.next.bump()
    }

    /// A resource name not used before: `prefix` and a number.
    fn name(&mut self, prefix: &str) -> String {
        self.names += 1;
        format!("{prefix}{}", self.names)
    }

    fn canvas(&mut self) -> &mut Canvas {
        self.canvases.last_mut().expect("a canvas is open while drawing")
    }

    fn content(&mut self) -> &mut Content {
        &mut self.canvas().content
    }

    fn page(&mut self, index: usize, page: &Page, page_ref: Ref, tree: Ref) {
        self.page = kurbo::Rect::new(0.0, 0.0, page.width, page.height);
        self.page_index = index;
        self.marks.push(Vec::new());
        // The page's own space is PDF's, y up; pictures draw y down from the top left.
        let flip = Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, page.height]);
        self.canvases.push(Canvas::new(flip));
        self.content().transform(coeffs(flip));
        let mut mcid = 0;
        for (mark, picture) in &page.parts {
            match *mark {
                Mark::Content(c) => {
                    if let Some(b) = picture.bounds() {
                        self.firsts.entry(c).or_insert((index, b.origin()));
                    }
                    self.content().begin_marked_content_with_properties(Name(b"P")).properties().identify(mcid);
                    self.marks[index].push((mcid, c));
                    mcid += 1;
                }
                Mark::Artifact(kind) => {
                    let kind = match kind {
                        ArtifactKind::Pagination => Some(ArtifactType::Pagination),
                        ArtifactKind::Layout => Some(ArtifactType::Layout),
                        ArtifactKind::Other => None,
                    };
                    match kind {
                        Some(k) => {
                            self.content().begin_marked_content_with_properties(Name(b"Artifact")).properties().artifact().kind(k);
                        }
                        None => {
                            self.content().begin_marked_content(Name(b"Artifact"));
                        }
                    }
                }
            }
            self.picture(picture, Affine::IDENTITY, Ink::Paint);
            self.content().end_marked_content();
        }
        let mut canvas = self.canvases.pop().expect("the page's canvas");
        let content_ref = self.alloc();
        let data = deflate(&std::mem::replace(&mut canvas.content, Content::new()).finish());
        self.pdf.stream(content_ref, &data).filter(Filter::FlateDecode);
        let mut p = self.pdf.page(page_ref);
        p.media_box(PdfRect::new(0.0, 0.0, page.width as f32, page.height as f32)).parent(tree).contents(content_ref);
        p.struct_parents(index as i32);
        resources(p.resources(), &canvas);
        p.finish();
    }

    /// Draws `picture` under `ctm` (its space to the page's).
    fn picture(&mut self, picture: &Picture, ctm: Affine, ink: Ink) {
        let lists = picture.lists();
        match classify(lists, 0, false) {
            Group::Plain => self.walk(lists, &mut 0, ctm, false, ink),
            Group::Masked => self.masked(lists, &mut 0, ctm, false, ink),
            Group::Raster => {
                let area = picture.bounds().map_or(self.page, |b| ctm.transform_rect_bbox(b).intersect(self.page));
                self.raster(area, ctm, ink, |cpu, target, res, t| {
                    cpu.render(target, res, &cpu::Frame { clear: None, draws: &[Draw { picture, transform: t }] })
                });
            }
        }
    }

    /// Writes `lists`' commands from `*i` as they are: to the end, or (`in_layer`) through the pop that ends the layer.
    fn walk(&mut self, lists: &Lists, i: &mut usize, ctm: Affine, in_layer: bool, ink: Ink) {
        let mut ctm = ctm;
        let mut saved: Vec<Affine> = Vec::new();
        while let Some(&cmd) = lists.cmds.get(*i) {
            *i += 1;
            match cmd {
                Cmd::PushTransform { transform } => {
                    let t = affine(lists.transforms[transform as usize]);
                    self.content().save_state().transform(coeffs(t));
                    saved.push(ctm);
                    ctm *= t;
                }
                Cmd::PushClip { clip } => {
                    self.content().save_state();
                    self.clip(lists, lists.clips[clip as usize]);
                    saved.push(ctm);
                }
                Cmd::PushLayer { .. } => self.layer(lists, i, ctm, ink),
                Cmd::Pop => match saved.pop() {
                    Some(c) => {
                        self.content().restore_state();
                        ctm = c;
                    }
                    None if in_layer => return,
                    None => {}
                },
                Cmd::Picture { child, transform } => {
                    let t = affine(lists.transforms[transform as usize]);
                    self.content().save_state().transform(coeffs(t));
                    let child = &lists.children[child as usize];
                    self.walk(child.lists(), &mut 0, ctm * t, false, ink);
                    self.content().restore_state();
                }
                _ => self.draw(lists, cmd, ctm, ink),
            }
        }
        for _ in saved {
            self.content().restore_state();
        }
    }

    /// Opens a form XObject's canvas, drawn under `ctm` (its space is the space where it will be drawn).
    fn begin_form(&mut self, ctm: Affine) {
        self.canvases.push(Canvas::new(ctm));
    }

    /// Closes the open form: an isolated transparency group (in RGB) covering the page, its XObject's reference.
    fn end_form(&mut self, ctm: Affine) -> Ref {
        let mut canvas = self.canvases.pop().expect("a form's canvas");
        let id = self.alloc();
        let data = deflate(&std::mem::replace(&mut canvas.content, Content::new()).finish());
        let b = ctm.inverse().transform_rect_bbox(self.page);
        let mut form = self.pdf.form_xobject(id, &data);
        form.filter(Filter::FlateDecode);
        form.bbox(PdfRect::new(b.x0 as f32, b.y0 as f32, b.x1 as f32, b.y1 as f32));
        form.group().transparency().isolated(true).color_space().device_rgb();
        resources(form.resources(), &canvas);
        form.finish();
        id
    }

    /// Draws form `id` here, with the graphics state `state` (if any) set first.
    fn draw_form(&mut self, id: Ref, state: Option<(String, Ref)>) {
        let name = self.name("X");
        let canvas = self.canvas();
        canvas.xobjects.insert(name.clone(), id);
        canvas.content.save_state();
        if let Some((s, r)) = state {
            canvas.states.insert(s.clone(), r);
            canvas.content.set_parameters(Name(s.as_bytes()));
        }
        canvas.content.x_object(Name(name.as_bytes()));
        canvas.content.restore_state();
    }

    /// A graphics state with fill and stroke alpha, a blend mode and a soft mask (any of them), written once.
    fn state(&mut self, alpha: Option<f32>, blend: Option<PdfBlend>, mask: Option<(MaskType, Ref)>) -> (String, Ref) {
        let key = format!("{alpha:?}{blend:?}{:?}", mask.map(|(t, r)| (t as u8, r.get())));
        if let Some(found) = self.states.get(&key) {
            return found.clone();
        }
        let id = self.alloc();
        let name = self.name("G");
        let mut s = self.pdf.ext_graphics(id);
        if let Some(a) = alpha {
            s.non_stroking_alpha(a).stroking_alpha(a);
        }
        if let Some(b) = blend {
            s.blend_mode(b);
        }
        if let Some((t, g)) = mask {
            s.soft_mask().subtype(t).group(g);
        }
        s.finish();
        self.states.insert(key, (name.clone(), id));
        (name, id)
    }

    /// Sets a graphics state in the current canvas.
    fn set_state(&mut self, state: (String, Ref)) {
        let canvas = self.canvas();
        canvas.states.insert(state.0.clone(), state.1);
        canvas.content.set_parameters(Name(state.0.as_bytes()));
    }

    /// Writes the layer whose push is at `lists.cmds[*i - 1]`, moving `*i` past its pop.
    fn layer(&mut self, lists: &Lists, i: &mut usize, ctm: Affine, ink: Ink) {
        let at = *i - 1;
        let Some(&Cmd::PushLayer { layer }) = lists.cmds.get(at) else { return };
        let rec = lists.layers[layer as usize];
        let group = classify(lists, *i, true);
        self.begin_form(ctm);
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
            self.raster(area, ctm, ink, |cpu, target, res, t| cpu.render_layer(target, res, lists, at, Transform::from_kurbo(t)));
        } else {
            let bounded = rec.bounds.map(|b| self.push_rect_clip(b.to_kurbo())).is_some();
            match group {
                Group::Masked => self.masked(lists, i, ctm, true, ink),
                _ => self.walk(lists, i, ctm, true, ink),
            }
            if bounded {
                self.content().restore_state();
            }
        }
        let form = self.end_form(ctm);
        let alpha = (rec.opacity < 1.0).then_some(rec.opacity.max(0.0));
        let state = (alpha.is_some() || native(rec.blend).is_some()).then(|| self.state(alpha, native(rec.blend), None));
        self.draw_form(form, state);
    }

    /// Draws `area` (page points) of what `render` draws there, rendered at 300 dots per inch and embedded as an image.
    /// `render` draws under the transform from the drawing's space to the image's pixels.
    fn raster(&mut self, area: kurbo::Rect, ctm: Affine, ink: Ink, render: impl FnOnce(&mut cpu::Renderer, &mut Target<'_>, &Resources, Affine)) {
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
        let image = self.rgba_image(&pixels, w, h, ink, self.interpolates(Sampling::Linear));
        // The image in page points, wherever the transform is now.
        let place = ctm.inverse() * Affine::translate((px.x0 / RASTER, px.y0 / RASTER)) * Affine::scale(1.0 / RASTER);
        self.draw_image(image, (w, h), place);
    }

    /// Draws image XObject `id` (`w × h` pixels) under `place` (its pixels, y down, to the current space).
    fn draw_image(&mut self, id: Ref, (w, h): (u32, u32), place: Affine) {
        let name = self.name("X");
        let canvas = self.canvas();
        canvas.xobjects.insert(name.clone(), id);
        // PDF draws an image into the unit square, its first row at the top (y = 1).
        let unit = Affine::new([f64::from(w), 0.0, 0.0, -f64::from(h), 0.0, f64::from(h)]);
        canvas.content.save_state().transform(coeffs(place * unit)).x_object(Name(name.as_bytes())).restore_state();
    }

    /// Writes a group (from `*i`, as [`Writer::walk`]) whose Porter-Duff draws and layers become soft masks.
    fn masked(&mut self, lists: &Lists, i: &mut usize, ctm: Affine, in_layer: bool, ink: Ink) {
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
        self.pieces(&acc, &ops, ctm, ink);
    }

    fn pieces(&mut self, pieces: &[Piece], ops: &[Op<'_>], ctm: Affine, ink: Ink) {
        for p in pieces {
            match p {
                Piece::Op(k) => self.op(&ops[*k], ctm, ink),
                Piece::Masked { content, mask, kind } => {
                    // Everywhere the page shows, in this space.
                    let area = ctm.inverse().transform_rect_bbox(self.page);
                    self.begin_form(ctm);
                    match *kind {
                        MaskKind::Alpha => self.pieces(mask, ops, ctm, ink),
                        MaskKind::AlphaInside(b) => {
                            let c = self.content();
                            rect_ops(c, area);
                            rect_ops(c, b);
                            c.set_fill_rgb(0.0, 0.0, 0.0).fill_even_odd();
                            self.push_rect_clip(b);
                            self.pieces(mask, ops, ctm, ink);
                            self.content().restore_state();
                        }
                        MaskKind::Inverse(black) => {
                            let c = self.content();
                            rect_ops(c, area);
                            c.set_fill_rgb(1.0, 1.0, 1.0).fill_nonzero();
                            self.pieces(mask, ops, ctm, black);
                        }
                    }
                    let mask_form = self.end_form(ctm);
                    self.begin_form(ctm);
                    self.pieces(content, ops, ctm, ink);
                    let content_form = self.end_form(ctm);
                    let mask_type = if matches!(kind, MaskKind::Inverse(_)) { MaskType::Luminosity } else { MaskType::Alpha };
                    let state = self.state(None, None, Some((mask_type, mask_form)));
                    self.draw_form(content_form, Some(state));
                }
            }
        }
    }

    /// A masked group's draw or layer, under its own transforms and clips; its Porter-Duff mode is what the masks did.
    fn op(&mut self, op: &Op<'_>, ctm: Affine, ink: Ink) {
        let mut ctm = ctm;
        self.content().save_state();
        for c in &op.context {
            match *c {
                Context::Transform(t) => {
                    self.content().transform(coeffs(t));
                    ctm *= t;
                }
                Context::Clip(lists, rec) => self.clip(lists, rec),
            }
        }
        match op.lists.cmds[op.at] {
            Cmd::PushLayer { .. } => self.layer(op.lists, &mut (op.at + 1), ctm, ink),
            cmd => self.draw(op.lists, cmd, ctm, ink),
        }
        self.content().restore_state();
    }

    /// Intersects the clip with `rec` (the caller saved the state).
    fn clip(&mut self, lists: &Lists, rec: ClipRec) {
        let c = self.content();
        match rec {
            ClipRec::Rect { rect, .. } => {
                rect_ops(c, rect.to_kurbo());
                c.clip_nonzero().end_path();
            }
            ClipRec::Path { path, even_odd } => {
                path_ops(c, &lists.verbs[path.verbs.range()], &lists.points[path.points.range()]);
                if even_odd {
                    c.clip_even_odd();
                } else {
                    c.clip_nonzero();
                }
                c.end_path();
            }
        }
    }

    /// Saves the state and clips to `r`; the caller restores it.
    fn push_rect_clip(&mut self, r: kurbo::Rect) {
        let c = self.content();
        c.save_state();
        rect_ops(c, r);
        c.clip_nonzero().end_path();
    }

    /// Writes one draw command under `ctm`.
    fn draw(&mut self, lists: &Lists, cmd: Cmd, ctm: Affine, ink: Ink) {
        let (paint, even_odd) = match cmd {
            Cmd::SolidRect { rect, color, blend, .. } => {
                if blend == BlendMode::Dst {
                    return;
                }
                self.content().save_state();
                if let Some(b) = native(blend) {
                    let s = self.state(None, Some(b), None);
                    self.set_state(s);
                }
                self.solid(color, ink);
                let c = self.content();
                rect_ops(c, rect.to_kurbo());
                c.fill_nonzero();
                c.restore_state();
                return;
            }
            Cmd::Rect { rect, paint } => {
                self.verbs.clear();
                self.points.clear();
                crate::cover::rect_path(rect, &mut self.verbs, &mut self.points);
                (paint, false)
            }
            Cmd::RRect { rrect, paint } => {
                let (rect, radii) = lists.rrects[rrect as usize];
                rrect_path(rect, radii, &mut self.verbs, &mut self.points);
                (paint, false)
            }
            Cmd::Frame { frame, paint } => {
                let (outer, inner) = lists.frames[frame as usize];
                self.verbs.clear();
                self.points.clear();
                frame_path(outer, inner, &mut self.verbs, &mut self.points);
                (paint, false)
            }
            Cmd::Path { path, paint } | Cmd::EvenOddPath { path, paint } => {
                self.verbs.clear();
                self.points.clear();
                self.verbs.extend_from_slice(&lists.verbs[path.verbs.range()]);
                self.points.extend_from_slice(&lists.points[path.points.range()]);
                (paint, matches!(cmd, Cmd::EvenOddPath { .. }))
            }
            Cmd::Glyphs { run, paint } => return self.glyphs(lists, run as usize, paint as usize, ctm, ink),
            Cmd::Image { image } => return self.image(lists, image as usize, ink),
            _ => return,
        };
        let rec = lists.paints[paint as usize];
        if rec.blend == BlendMode::Dst {
            return;
        }
        let (verbs, points) = (std::mem::take(&mut self.verbs), std::mem::take(&mut self.points));
        self.fill(lists, paint as usize, ink, ctm, |c| path_ops(c, &verbs, &points), even_odd, bounds_of(&points));
        (self.verbs, self.points) = (verbs, points);
    }

    /// Sets the fill colour (and its alpha) to `c`, or black for a mask's ink.
    fn solid(&mut self, c: PremulColor, ink: Ink) {
        let alpha = f32::from(c.a) / 255.0;
        let (rgb, alpha) = match ink {
            Ink::Black => ([0.0; 3], 1.0),
            Ink::BlackAlpha => ([0.0; 3], alpha),
            Ink::Paint => {
                let un = |v: u8| if c.a == 0 { 0.0 } else { (f32::from(v) / f32::from(c.a)).min(1.0) };
                ([un(c.r), un(c.g), un(c.b)], alpha)
            }
        };
        if alpha < 1.0 {
            let s = self.state(Some(alpha), None, None);
            self.set_state(s);
        }
        self.content().set_fill_rgb(rgb[0], rgb[1], rgb[2]);
    }

    /// Fills the path `shape` writes with paint `index` (under `ctm`; `bounds` is the path's, in its space), in a saved
    /// state of its own.
    #[allow(clippy::too_many_arguments, reason = "one fill's shape, paint and place")]
    fn fill(&mut self, lists: &Lists, index: usize, ink: Ink, ctm: Affine, shape: impl Fn(&mut Content), even_odd: bool, bounds: kurbo::Rect) {
        let rec = lists.paints[index];
        self.content().save_state();
        if let Some(b) = native(rec.blend) {
            let s = self.state(None, Some(b), None);
            self.set_state(s);
        }
        let fill = |c: &mut Content| {
            if even_odd {
                c.fill_even_odd();
            } else {
                c.fill_nonzero();
            }
        };
        match rec.brush {
            BrushRec::Solid(c) => {
                self.solid(c, ink);
                let content = self.content();
                shape(content);
                fill(content);
            }
            BrushRec::Pattern { .. } if ink == Ink::Black => {
                self.content().set_fill_rgb(0.0, 0.0, 0.0);
                let content = self.content();
                shape(content);
                fill(content);
            }
            BrushRec::Pattern { image, transform, sampling } => {
                let Some(e) = self.embedded(image, &[], ink, sampling) else {
                    self.content().restore_state();
                    return;
                };
                let name = self.name("P");
                let id = self.alloc();
                let base = self.canvas().base;
                let matrix = base.inverse() * ctm * affine(transform);
                let (uw, uh) = e.upright;
                let mut tile = Content::new();
                let unit = Affine::new([f64::from(e.stored.0), 0.0, 0.0, -f64::from(e.stored.1), 0.0, f64::from(e.stored.1)]);
                tile.transform(coeffs(e.orient * unit)).x_object(Name(b"I"));
                let data = tile.finish();
                let mut p = self.pdf.tiling_pattern(id, &data);
                p.paint_type(PaintType::Colored).tiling_type(TilingType::ConstantSpacing);
                p.bbox(PdfRect::new(0.0, 0.0, uw as f32, uh as f32)).x_step(uw as f32).y_step(uh as f32).matrix(coeffs(matrix));
                p.resources().x_objects().pair(Name(b"I"), e.object);
                p.finish();
                let canvas = self.canvas();
                canvas.patterns.insert(name.clone(), id);
                canvas.content.set_fill_color_space(pdf_writer::types::ColorSpaceOperand::Pattern);
                canvas.content.set_fill_pattern(None, Name(name.as_bytes()));
                shape(&mut canvas.content);
                fill(&mut canvas.content);
            }
            _ => {
                // A gradient: its shading painted inside the shape, under the gradient's own transform; any alpha in its
                // stops a soft mask of the same gradient.
                let content = self.content();
                shape(content);
                if even_odd {
                    content.clip_even_odd();
                } else {
                    content.clip_nonzero();
                }
                content.end_path();
                let gradient = gradient_of(lists, rec.brush);
                let inside = affine(gradient.transform).inverse().transform_rect_bbox(bounds);
                let alpha = lists.stops[gradient.stops.range()].iter().any(|s| s.color.a < 255) || ink == Ink::BlackAlpha;
                if alpha && ink != Ink::Black {
                    let local_ctm = ctm;
                    self.begin_form(local_ctm);
                    self.content().transform(coeffs(affine(gradient.transform)));
                    let sh = self.shading(lists, &gradient, inside, Channel::Alpha);
                    self.paint_shading(sh);
                    let mask = self.end_form(local_ctm);
                    let s = self.state(None, None, Some((MaskType::Luminosity, mask)));
                    self.set_state(s);
                }
                self.content().transform(coeffs(affine(gradient.transform)));
                let channel = if ink == Ink::Paint { Channel::Colour } else { Channel::Black };
                let sh = self.shading(lists, &gradient, inside, channel);
                self.paint_shading(sh);
            }
        }
        self.content().restore_state();
    }

    fn paint_shading(&mut self, sh: Ref) {
        let name = self.name("S");
        let canvas = self.canvas();
        canvas.shadings.insert(name.clone(), sh);
        canvas.content.shading(Name(name.as_bytes()));
    }

    /// A shading for `g` covering `inside` (in the gradient's space): its colours, its alpha as grey, or black.
    fn shading(&mut self, lists: &Lists, g: &Gradient, inside: kurbo::Rect, channel: Channel) -> Ref {
        let stops: Vec<([f32; 3], f32)> = lists.stops[g.stops.range()]
            .iter()
            .map(|s: &Stop| {
                let c = match channel {
                    Channel::Colour => [s.color.r, s.color.g, s.color.b].map(|v| f32::from(v) / 255.0),
                    Channel::Alpha => [f32::from(s.color.a) / 255.0; 3],
                    Channel::Black => [0.0; 3],
                };
                (c, s.offset.clamp(0.0, 1.0))
            })
            .collect();
        let id = self.alloc();
        match g.kind {
            GradientKind::Sweep { center, start, end } => {
                let code = sweep_function(&stops, center, start, end, g.extend);
                let f = self.alloc();
                let domain = [inside.x0, inside.x1, inside.y0, inside.y1].map(|v| v as f32);
                self.pdf.post_script_function(f, code.as_bytes()).domain(domain).range([0.0, 1.0, 0.0, 1.0, 0.0, 1.0]);
                let mut sh = self.pdf.function_shading(id);
                sh.shading_type(FunctionShadingType::Function).color_space().device_rgb();
                sh.domain(domain).function(f);
                sh.finish();
            }
            GradientKind::Linear { start, end } | GradientKind::Radial { start, end, .. } => {
                // The span of t the shape needs: the corners' projection onto the axis (radial: their distance out).
                let corners = [(inside.x0, inside.y0), (inside.x1, inside.y0), (inside.x0, inside.y1), (inside.x1, inside.y1)];
                let (lo, hi) = match (g.kind, g.extend) {
                    (_, Extend::Clamp) => (0.0, 1.0),
                    (GradientKind::Linear { .. }, _) => {
                        let d = (f64::from(end.x - start.x), f64::from(end.y - start.y));
                        let len2 = (d.0 * d.0 + d.1 * d.1).max(1e-12);
                        let ts = corners.map(|(x, y)| ((x - f64::from(start.x)) * d.0 + (y - f64::from(start.y)) * d.1) / len2);
                        (
                            ts.iter().copied().fold(f64::INFINITY, f64::min).floor(),
                            ts.iter().copied().fold(f64::NEG_INFINITY, f64::max).ceil().max(1.0),
                        )
                    }
                    (GradientKind::Radial { r0, r1, .. }, _) => {
                        let far = corners.iter().map(|&(x, y)| (x - f64::from(end.x)).hypot(y - f64::from(end.y))).fold(0.0, f64::max);
                        let dr = f64::from(r1 - r0).abs().max(1e-6);
                        (0.0, ((far + f64::from(start.x - end.x).hypot(f64::from(start.y - end.y))) / dr).ceil().max(1.0))
                    }
                    _ => (0.0, 1.0),
                };
                let f = self.stitched(&stops, lo, hi, g.extend);
                let mut sh = self.pdf.function_shading(id);
                let at = |t: f64, a: f32, b: f32| (f64::from(a) + t * f64::from(b - a)) as f32;
                match g.kind {
                    GradientKind::Linear { .. } => {
                        sh.shading_type(FunctionShadingType::Axial);
                        sh.coords([at(lo, start.x, end.x), at(lo, start.y, end.y), at(hi, start.x, end.x), at(hi, start.y, end.y)]);
                    }
                    GradientKind::Radial { r0, r1, .. } => {
                        sh.shading_type(FunctionShadingType::Radial);
                        sh.coords([start.x, start.y, r0, at(hi, start.x, end.x), at(hi, start.y, end.y), at(hi, r0, r1).max(0.0)]);
                    }
                    GradientKind::Sweep { .. } => unreachable!("handled above"),
                }
                sh.color_space().device_rgb();
                sh.insert(Name(b"Domain")).array().items([lo as f32, hi as f32]);
                sh.function(f).extend([true, true]);
                sh.finish();
            }
        }
        id
    }

    /// A function of t from `lo` to `hi` giving the stops' colours: once over 0 to 1 (clamped past the ends), or
    /// repeated or mirrored over each unit.
    fn stitched(&mut self, stops: &[([f32; 3], f32)], lo: f64, hi: f64, extend: Extend) -> Ref {
        // One unit's pieces (start, end, colours): from stop to stop, the first and last colours continued to 0 and 1.
        let first = stops.first().map_or([0.0; 3], |s| s.0);
        let mut pieces: Vec<(f32, f32, [f32; 3], [f32; 3])> = Vec::new();
        let (mut at, mut colour) = (0.0f32, first);
        for &(c, o) in stops {
            if o > at {
                pieces.push((at, o, colour, c));
                at = o;
            }
            colour = c;
        }
        if at < 1.0 || pieces.is_empty() {
            pieces.push((at.min(1.0), 1.0, colour, colour));
        }
        let (mut functions, mut bounds, mut encode) = (Vec::new(), Vec::new(), Vec::new());
        let units = if extend == Extend::Clamp { 0..1 } else { (lo as i64)..(hi as i64) };
        for k in units {
            let reverse = extend == Extend::Mirror && k.rem_euclid(2) == 1;
            let mut unit: Vec<(f32, [f32; 3], [f32; 3])> = if reverse {
                pieces.iter().rev().map(|&(s, _, c0, c1)| (1.0 - s, c1, c0)).collect()
            } else {
                pieces.iter().map(|&(_, e, c0, c1)| (e, c0, c1)).collect()
            };
            for (end, c0, c1) in unit.drain(..) {
                let f = self.alloc();
                self.pdf.exponential_function(f).domain([0.0, 1.0]).c0(c0).c1(c1).n(1.0);
                functions.push(f);
                bounds.push(k as f32 + end);
                encode.extend([0.0, 1.0]);
            }
        }
        bounds.pop();
        let (lo, hi) = if extend == Extend::Clamp { (0.0, 1.0) } else { (lo as f32, hi as f32) };
        let id = self.alloc();
        self.pdf.stitching_function(id).domain([lo, hi]).functions(functions).bounds(bounds).encode(encode);
        id
    }

    fn glyphs(&mut self, lists: &Lists, run: usize, paint: usize, ctm: Affine, ink: Ink) {
        let run = lists.runs[run];
        let rec = lists.paints[paint];
        if rec.blend == BlendMode::Dst {
            return;
        }
        let glyphs = &lists.glyphs[run.glyphs.range()];
        let Some(first) = glyphs.first() else { return };
        let size = run.size;
        let text = std::str::from_utf8(&lists.text.as_bytes()[run.text.range()]).unwrap_or("");
        let clusters = &lists.clusters[run.clusters.range()];
        let end = |c: u32| clusters.iter().copied().filter(|&d| d > c).min().map_or(text.len(), |d| d as usize);
        let piece = |k: usize| -> &str {
            match clusters.get(k) {
                Some(&c) => text.get(c as usize..end(c)).unwrap_or(""),
                None if k == 0 => text,
                None => "",
            }
        };
        let Some(face) = self.face(run.font) else { return };
        let solid = match rec.brush {
            BrushRec::Solid(c) => Some(c),
            _ => None,
        };
        // Outlines, with the characters as ActualText: a gradient's text, and emboldened Type 3 text.
        if solid.is_none() || (run.synthesis.embolden && face == FaceKind::Outlines) {
            return self.glyph_outlines(lists, run, paint, ctm, ink, text);
        }
        let colour = solid.expect("solid paint");
        let fg = unpremultiply(colour);
        // Each glyph's font and code; whether its font's map gives back its characters.
        let mut placed: Vec<(Option<usize>, u32, f32)> = Vec::with_capacity(glyphs.len());
        let mut round_trips = true;
        for (k, g) in glyphs.iter().enumerate() {
            let kind = self.res.fonts.kind(run.font, g.id);
            let chars = piece(k).to_string();
            let (font, code, width, mapped) = match (face, kind) {
                (FaceKind::Cid, GlyphKind::Outline) => {
                    let width = self.advance(run.font, g.id);
                    let entry = self.cid.entry(run.font).or_default();
                    let mapped = entry.glyphs.entry(g.id as u16).or_insert((width, chars.clone())).1.clone();
                    (None, g.id, width, mapped)
                }
                (_, GlyphKind::Outline) => {
                    let (t, code) = self.type3_code(run.font, None, g.id, &chars);
                    (Some(t), u32::from(code), self.advance(run.font, g.id), self.type3[t].glyphs[code as usize].1.clone())
                }
                _ => {
                    let (t, code) = self.type3_code(run.font, Some((fg, ink)), g.id, &chars);
                    (Some(t), u32::from(code), self.advance(run.font, g.id), self.type3[t].glyphs[code as usize].1.clone())
                }
            };
            round_trips &= mapped == chars;
            placed.push((font, code, width));
        }
        self.content().save_state();
        if let Some(b) = native(rec.blend) {
            let s = self.state(None, Some(b), None);
            self.set_state(s);
        }
        let actual = !round_trips && !text.is_empty();
        if actual {
            self.content().begin_marked_content_with_properties(Name(b"Span")).properties().actual_text(TextStr(text));
        }
        self.solid(colour, ink);
        let skew = run.synthesis.skew_x;
        let bold = run.synthesis.embolden;
        let c = self.content();
        if bold {
            let ink_rgb = |v: u8| if colour.a == 0 { 0.0 } else { (f32::from(v) / f32::from(colour.a)).min(1.0) };
            match ink {
                Ink::Paint => c.set_stroke_rgb(ink_rgb(colour.r), ink_rgb(colour.g), ink_rgb(colour.b)),
                _ => c.set_stroke_rgb(0.0, 0.0, 0.0),
            };
            c.set_line_width(size * embolden_em(size)).set_line_join(pdf_writer::types::LineJoinStyle::MiterJoin);
        }
        c.begin_text();
        if bold {
            c.set_text_rendering_mode(pdf_writer::types::TextRenderingMode::FillStroke);
        }
        c.set_text_matrix([1.0, 0.0, -skew, -1.0, first.x, run.baseline]);
        let mut k = 0;
        while k < placed.len() {
            let font = placed[k].0;
            let name = match font {
                None => self.cid_name(run.font),
                Some(t) => self.type3[t].name.clone(),
            };
            let object = match font {
                None => self.cid[&run.font].object.expect("a CID font's object"),
                Some(t) => self.type3[t].object,
            };
            self.canvas().fonts.insert(name.clone(), object);
            let c = self.content();
            c.set_font(Name(name.as_bytes()), size);
            let mut tj = c.show_positioned();
            let mut items = tj.items();
            while k < placed.len() && placed[k].0 == font {
                let (_, code, width) = placed[k];
                let bytes: Vec<u8> = if font.is_none() { (code as u16).to_be_bytes().to_vec() } else { vec![code as u8] };
                items.show(Str(&bytes));
                if let Some(next) = glyphs.get(k + 1) {
                    let gap = width - (next.x - glyphs[k].x) * 1000.0 / size;
                    if gap.abs() > 1e-3 {
                        items.adjust(gap);
                    }
                }
                k += 1;
            }
            drop(items);
            tj.finish();
        }
        let c = self.content();
        c.end_text();
        if actual {
            c.end_marked_content();
        }
        c.restore_state();
    }

    /// A run drawn as its glyphs' outlines (filled with its paint, emboldened and skewed as the CPU backend draws
    /// them), its characters as ActualText.
    #[allow(clippy::too_many_arguments, reason = "one run's place and paint")]
    fn glyph_outlines(&mut self, lists: &Lists, run: RunRec, paint: usize, ctm: Affine, ink: Ink, text: &str) {
        let glyphs = &lists.glyphs[run.glyphs.range()];
        let size = f64::from(run.size);
        let skew = f64::from(run.synthesis.skew_x);
        let base = f64::from(run.baseline);
        let mut path = BezPath::new();
        let mut one = BezPath::new();
        for g in glyphs {
            one.truncate(0);
            if !self.res.fonts.em_outline(run.font, g.id, &mut one) {
                continue;
            }
            let t = Affine::new([1.0, 0.0, skew, 1.0, -skew * base, 0.0]) * Affine::translate((f64::from(g.x), base)) * Affine::scale(size);
            path.extend(one.iter().map(|el| t * el));
        }
        if run.synthesis.embolden {
            // Skia's fake bold: the outline stroked by its share of the em, round about the joins.
            let stroke = kurbo::Stroke::new(size * f64::from(embolden_em(run.size))).with_join(kurbo::Join::Miter);
            let outline = kurbo::stroke(path.iter(), &stroke, &kurbo::StrokeOpts::default(), 0.01);
            path.extend(outline.iter());
        }
        let actual = !text.is_empty();
        if actual {
            self.content().begin_marked_content_with_properties(Name(b"Span")).properties().actual_text(TextStr(text));
        }
        let (verbs, points) = path_lists(&path);
        let bounds = path.bounding_box();
        self.fill(lists, paint, ink, ctm, |c| path_ops(c, &verbs, &points), false, bounds);
        if actual {
            self.content().end_marked_content();
        }
    }

    /// The width of `glyph` in 1000 units per em, at the face's coordinates.
    fn advance(&self, font: FontId, glyph: u32) -> f32 {
        let Some((f, location)) = self.res.fonts.font_ref(font) else { return 0.0 };
        f.glyph_metrics(skrifa::instance::Size::new(1000.0), location).advance_width(skrifa::GlyphId::new(glyph)).unwrap_or(0.0)
    }

    /// How a face's glyphs are written: embedded when it is a TrueType face at its default instance.
    fn face(&mut self, font: FontId) -> Option<FaceKind> {
        let res = self.res;
        *self.faces.entry(font).or_insert_with(|| {
            let (data, index, variations) = res.fonts.source(font)?;
            let f = skrifa::FontRef::from_index((**data).as_ref(), index).ok()?;
            let subsets = subset_glyf((**data).as_ref(), index, &std::collections::BTreeSet::new(), false).is_some();
            Some(if variations.is_empty() && f.glyf().is_ok() && subsets { FaceKind::Cid } else { FaceKind::Outlines })
        })
    }

    /// The resource name of `font`'s CID font, its object allocated on first use.
    fn cid_name(&mut self, font: FontId) -> String {
        if self.cid.get(&font).and_then(|c| c.object).is_none() {
            let object = self.alloc();
            let name = self.name("F");
            let entry = self.cid.entry(font).or_default();
            entry.object = Some(object);
            entry.name = name;
        }
        self.cid[&font].name.clone()
    }

    /// `glyph`'s Type 3 font and code, given a code on first use (its characters, `chars`, kept for the font's map).
    fn type3_code(&mut self, face: FontId, colour: Option<(Color, Ink)>, glyph: u32, chars: &str) -> (usize, u8) {
        if let Some(&found) = self.type3_codes.get(&((face, colour), glyph)) {
            return found;
        }
        let open = self.type3_open.get(&(face, colour)).copied().filter(|&t| self.type3[t].glyphs.len() < 256);
        let t = match open {
            Some(t) => t,
            None => {
                let (object, name) = (self.alloc(), self.name("T"));
                self.type3.push(Type3 { name, object, face, colour, glyphs: Vec::new() });
                self.type3_open.insert((face, colour), self.type3.len() - 1);
                self.type3.len() - 1
            }
        };
        let code = self.type3[t].glyphs.len() as u8;
        self.type3[t].glyphs.push((glyph, chars.to_string()));
        self.type3_codes.insert(((face, colour), glyph), (t, code));
        (t, code)
    }

    fn image(&mut self, lists: &Lists, index: usize, ink: Ink) {
        let rec = lists.images[index];
        let filters = &lists.color_filters[rec.color_filters.range()];
        let Some(e) = self.embedded(rec.image, filters, ink, rec.sampling) else { return };
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
            self.push_rect_clip(dst.abs());
        }
        self.draw_image(e.object, e.stored, place);
        if clipped {
            self.content().restore_state();
        }
    }

    /// Whether an image drawn with `sampling` asks viewers to interpolate it: never under PDF/A, which forbids it.
    fn interpolates(&self, sampling: Sampling) -> bool {
        !self.archival && sampling != Sampling::Nearest
    }

    /// Image `id` with `filters` applied, embedded once: an unfiltered JPEG as its file, anything else decoded.
    fn embedded(&mut self, id: ImageId, filters: &[ColorFilter], ink: Ink, sampling: Sampling) -> Option<Embedded> {
        let interpolate = self.interpolates(sampling);
        let key = (id, filter_hash(filters, ink, interpolate));
        if let Some(found) = self.images.get(&key) {
            return *found;
        }
        let res = self.res;
        let made = (|| {
            if filters.is_empty()
                && ink == Ink::Paint
                && let Some((data, info)) = res.images.encoded(id)
                && info.format == ImageFormat::Jpeg
                && let Some(components) = jpeg_components((**data).as_ref())
            {
                let stored = if info.orientation >= 5 { (info.height, info.width) } else { (info.width, info.height) };
                let object = self.alloc();
                let mut x = self.pdf.image_xobject(object, (**data).as_ref());
                x.filter(Filter::DctDecode);
                x.width(stored.0 as i32).height(stored.1 as i32).bits_per_component(8);
                x.color_space_name(Name(if components == 1 { b"DeviceGray" } else { b"DeviceRGB" }));
                if interpolate {
                    x.interpolate(true);
                }
                x.finish();
                let orient = orientation(info.orientation, f64::from(stored.0), f64::from(stored.1));
                return Some(Embedded { object, stored, upright: (info.width, info.height), orient });
            }
            let info = res.images.info(id)?;
            let (w, h) = (info.width, info.height);
            let mut pixels = vec![0u8; w as usize * h as usize * 4];
            {
                // The image drawn at its size, through its colour filters, by the CPU backend.
                let mut rec = Recorder::new();
                let paint = ImagePaint { sampling: Sampling::Nearest, color_filters: filters };
                rec.image(id, None, kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)), &paint);
                let picture = Recorder::finish(&mut rec);
                let mut target = Target::new(&mut pixels, w, h, w as usize * 4, Format::Rgba8).ok()?;
                let draws = [Draw { picture: &picture, transform: Affine::IDENTITY }];
                self.cpu.render(&mut target, res, &cpu::Frame { clear: None, draws: &draws });
            }
            let object = self.rgba_image(&pixels, w, h, ink, interpolate);
            Some(Embedded { object, stored: (w, h), upright: (w, h), orient: Affine::IDENTITY })
        })();
        self.images.insert(key, made);
        made
    }

    /// Premultiplied RGBA pixels as an image XObject: unpremultiplied colour (black for a mask's ink) and, when any
    /// pixel isn't opaque, a soft mask of their alpha.
    fn rgba_image(&mut self, pixels: &[u8], w: u32, h: u32, ink: Ink, interpolate: bool) -> Ref {
        let mut colour = Vec::with_capacity(w as usize * h as usize * 3);
        let mut alpha = Vec::with_capacity(w as usize * h as usize);
        for p in pixels.as_chunks::<4>().0 {
            let a = p[3];
            let un = |v: u8| if a == 0 { 0 } else { ((u32::from(v) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8 };
            if ink == Ink::Paint {
                colour.extend([un(p[0]), un(p[1]), un(p[2])]);
            } else {
                colour.extend([0, 0, 0]);
            }
            alpha.push(a);
        }
        let smask = alpha.iter().any(|&a| a != 255).then(|| {
            let id = self.alloc();
            let data = deflate(&alpha);
            let mut x = self.pdf.image_xobject(id, &data);
            x.filter(Filter::FlateDecode);
            x.width(w as i32).height(h as i32).bits_per_component(8).color_space_name(Name(b"DeviceGray"));
            if interpolate {
                x.interpolate(true);
            }
            x.finish();
            id
        });
        let id = self.alloc();
        let data = deflate(&colour);
        let mut x = self.pdf.image_xobject(id, &data);
        x.filter(Filter::FlateDecode);
        x.width(w as i32).height(h as i32).bits_per_component(8).color_space_name(Name(b"DeviceRGB"));
        if interpolate {
            x.interpolate(true);
        }
        if let Some(m) = smask {
            x.s_mask(m);
        }
        x.finish();
        id
    }

    /// Writes the fonts the pages used: the CID fonts subset, the Type 3 fonts' glyphs.
    fn fonts_out(&mut self) {
        let mut cid: Vec<(FontId, CidFont)> = std::mem::take(&mut self.cid).into_iter().collect();
        cid.sort_by_key(|(f, _)| *f);
        for (font, c) in cid {
            let Some(object) = c.object else { continue };
            self.cid_font(font, object, &c);
        }
        for t in std::mem::take(&mut self.type3) {
            self.type3_font(&t);
        }
    }

    /// Writes `font`'s CID font (Type 0, Identity encoding, glyph ids as CIDs), its glyphs subset. (Only faces that
    /// subset are written so: [`Writer::face`].)
    fn cid_font(&mut self, font: FontId, object: Ref, c: &CidFont) {
        let res = self.res;
        let Some((data, index, _)) = res.fonts.source(font) else { return };
        let used: std::collections::BTreeSet<u16> = c.glyphs.keys().copied().collect();
        let subset = subset_glyf((**data).as_ref(), index, &used, false).unwrap_or_default();
        let Some((f, location)) = res.fonts.font_ref(font) else { return };
        let m = f.metrics(skrifa::instance::Size::new(1000.0), location);
        let name = base_name(&f, font, &used);
        let (cid_ref, descriptor, file, cmap) = (self.alloc(), self.alloc(), self.alloc(), self.alloc());
        self.pdf
            .type0_font(object)
            .base_font(Name(name.as_bytes()))
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(cid_ref)
            .to_unicode(cmap);
        let mut cf = self.pdf.cid_font(cid_ref);
        cf.subtype(CidFontType::Type2).base_font(Name(name.as_bytes()));
        cf.system_info(SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 });
        cf.font_descriptor(descriptor).default_width(0.0).cid_to_gid_map_predefined(Name(b"Identity"));
        {
            let mut widths = cf.widths();
            for (&g, &(w, _)) in &c.glyphs {
                widths.consecutive(g, [w]);
            }
        }
        cf.finish();
        self.descriptor(descriptor, &name, &m, f.post().is_ok_and(|p| p.is_fixed_pitch() != 0)).font_file2(file);
        let data = deflate(&subset);
        self.pdf.stream(file, &data).filter(Filter::FlateDecode).pair(Name(b"Length1"), subset.len() as i32);
        let mut u = UnicodeCmap::new(Name(b"Custom"), SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"UCS"), supplement: 0 });
        for (&g, (_, s)) in &c.glyphs {
            if !s.is_empty() {
                u.pair_with_multiple(g, s.chars());
            }
        }
        let data = deflate(&u.finish());
        self.pdf.cmap(cmap, &data).filter(Filter::FlateDecode);
    }

    /// A font descriptor from the face's metrics (1000 units per em).
    fn descriptor(&mut self, id: Ref, name: &str, m: &skrifa::metrics::Metrics, fixed: bool) -> pdf_writer::writers::FontDescriptor<'_> {
        let b = m.bounds.unwrap_or_default();
        let mut flags = FontFlags::NON_SYMBOLIC;
        if fixed {
            flags |= FontFlags::FIXED_PITCH;
        }
        if m.italic_angle != 0.0 {
            flags |= FontFlags::ITALIC;
        }
        let mut d = self.pdf.font_descriptor(id);
        d.name(Name(name.as_bytes())).flags(flags).bbox(PdfRect::new(b.x_min, b.y_min, b.x_max, b.y_max));
        d.italic_angle(m.italic_angle).ascent(m.ascent).descent(m.descent).cap_height(m.cap_height.unwrap_or(m.ascent)).stem_v(80.0);
        d
    }

    /// Writes a Type 3 font: each glyph's outline (1000 units per em, y up), or its colour image.
    fn type3_font(&mut self, t: &Type3) {
        let res = self.res;
        let mut procs: Vec<(String, Ref)> = Vec::new();
        let mut widths: Vec<f32> = Vec::new();
        let mut images: Vec<(String, Ref)> = Vec::new();
        let mut bbox = kurbo::Rect::ZERO;
        let mut unicode = UnicodeCmap::new(Name(b"Custom"), SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"UCS"), supplement: 0 });
        for (code, (glyph, chars)) in t.glyphs.iter().enumerate() {
            let width = self.advance(t.face, *glyph);
            widths.push(width);
            if !chars.is_empty() {
                unicode.pair_with_multiple(code as u8, chars.chars());
            }
            let mut c = Content::new();
            match t.colour {
                None => {
                    let mut outline = BezPath::new();
                    res.fonts.em_outline(t.face, *glyph, &mut outline);
                    let up = Affine::scale_non_uniform(1000.0, -1000.0);
                    let path: BezPath = outline.iter().map(|el| up * el).collect();
                    let b = if path.elements().is_empty() { kurbo::Rect::ZERO } else { path.bounding_box() };
                    bbox = bbox.union(b);
                    c.start_shape_glyph(width, b.x0 as f32, b.y0 as f32, b.x1 as f32, b.y1 as f32);
                    let (verbs, points) = path_lists(&path);
                    path_ops(&mut c, &verbs, &points);
                    c.fill_nonzero();
                }
                Some((fg, ink)) => {
                    c.start_color_glyph(width);
                    let mut em: Option<kurbo::Rect> = None;
                    res.fonts.boxes(t.face, std::iter::once(*glyph), |_, b| em = b);
                    if let Some(em) = em {
                        let px = Affine::scale(COLOUR_GLYPH_PX).transform_rect_bbox(em).expand();
                        let (w, h) = (px.width() as u32, px.height() as u32);
                        if w > 0 && h > 0 {
                            let mut pixels = vec![0u8; w as usize * h as usize * 4];
                            let mut rec = Recorder::new();
                            let run = GlyphRun {
                                font: t.face,
                                size: COLOUR_GLYPH_PX,
                                baseline: -px.y0,
                                glyphs: &[(*glyph, -px.x0)],
                                synthesis: Synthesis::default(),
                            };
                            rec.glyphs(&res.fonts, &run, fg);
                            let picture = Recorder::finish(&mut rec);
                            if let Ok(mut target) = Target::new(&mut pixels, w, h, w as usize * 4, Format::Rgba8) {
                                let draws = [Draw { picture: &picture, transform: Affine::IDENTITY }];
                                self.cpu.render(&mut target, res, &cpu::Frame { clear: None, draws: &draws });
                            }
                            let image = self.rgba_image(&pixels, w, h, ink, !self.archival);
                            let name = self.name("I");
                            images.push((name.clone(), image));
                            // The image's pixels in glyph space: 1000 units per em, y up.
                            let k = 1000.0 / COLOUR_GLYPH_PX;
                            let (x0, y0, x1, y1) = (px.x0 * k, -px.y1 * k, px.x1 * k, -px.y0 * k);
                            bbox = bbox.union(kurbo::Rect::new(x0, y0, x1, y1));
                            c.save_state().transform([(x1 - x0) as f32, 0.0, 0.0, (y1 - y0) as f32, x0 as f32, y0 as f32]);
                            c.x_object(Name(name.as_bytes())).restore_state();
                        }
                    }
                }
            }
            let id = self.alloc();
            let data = deflate(&c.finish());
            self.pdf.stream(id, &data).filter(Filter::FlateDecode);
            procs.push((format!("g{code}"), id));
        }
        let cmap = self.alloc();
        let data = deflate(&unicode.finish());
        self.pdf.cmap(cmap, &data).filter(Filter::FlateDecode);
        let mut f = self.pdf.type3_font(t.object);
        f.bbox(PdfRect::new(bbox.x0 as f32, bbox.y0 as f32, bbox.x1 as f32, bbox.y1 as f32));
        f.matrix([0.001, 0.0, 0.0, 0.001, 0.0, 0.0]);
        f.char_procs().pairs(procs.iter().map(|(n, r)| (Name(n.as_bytes()), *r)));
        f.encoding_custom().differences().consecutive(0, procs.iter().map(|(n, _)| Name(n.as_bytes())));
        f.first_char(0).last_char(procs.len().saturating_sub(1) as u8).widths(widths);
        f.resources().x_objects().pairs(images.iter().map(|(n, r)| (Name(n.as_bytes()), *r)));
        f.to_unicode(cmap);
        f.finish();
    }

    /// Writes the structure tree: the document's element over `structure`, each element's marked content by page.
    fn structure(&mut self, structure: &[Element], pages: &[Ref]) -> Ref {
        let root = self.alloc();
        let document = self.alloc();
        // Each content id's marked content: (page, marked-content id).
        let mut content: FastMap<u32, Vec<(usize, i32)>> = FastMap::default();
        for (page, marks) in self.marks.iter().enumerate() {
            for &(mcid, c) in marks {
                content.entry(c).or_default().push((page, mcid));
            }
        }
        let mut parents: Vec<Vec<(i32, Ref)>> = vec![Vec::new(); pages.len()];
        let mut kids = Vec::new();
        for e in structure {
            kids.push(self.element(e, document, pages, &content, &mut parents));
        }
        let mut d = self.pdf.struct_element(document);
        d.kind(StructRole::Document).parent(root);
        {
            let mut children = d.children();
            for &k in &kids {
                children.struct_element(k);
            }
        }
        d.finish();
        // Each page's marked content's elements, in marked-content order: the parent tree's entries.
        let arrays: Vec<Ref> = parents
            .iter_mut()
            .map(|list| {
                list.sort_by_key(|(m, _)| *m);
                let id = self.alloc();
                self.pdf.indirect(id).array().items(list.iter().map(|(_, r)| *r));
                id
            })
            .collect();
        let mut r = self.pdf.indirect(root).start::<pdf_writer::writers::StructTreeRoot>();
        r.child(document);
        {
            let mut tree = r.parent_tree();
            let mut nums = tree.nums();
            for (page, &a) in arrays.iter().enumerate() {
                nums.insert(page as i32, a);
            }
        }
        r.parent_tree_next_key(pages.len() as i32);
        r.finish();
        root
    }

    fn element(
        &mut self,
        e: &Element,
        parent: Ref,
        pages: &[Ref],
        content: &FastMap<u32, Vec<(usize, i32)>>,
        parents: &mut [Vec<(i32, Ref)>],
    ) -> Ref {
        let id = self.alloc();
        let kids: Vec<Ref> = e.kids.iter().map(|k| self.element(k, id, pages, content, parents)).collect();
        let marked = e.content.and_then(|c| content.get(&c)).cloned().unwrap_or_default();
        for &(page, mcid) in &marked {
            parents[page].push((mcid, id));
        }
        let role = match &e.kind {
            Kind::Paragraph => StructRole::P,
            Kind::Heading(1) => StructRole::H1,
            Kind::Heading(2) => StructRole::H2,
            Kind::Heading(3) => StructRole::H3,
            Kind::Heading(4) => StructRole::H4,
            Kind::Heading(5) => StructRole::H5,
            Kind::Heading(_) => StructRole::H6,
            Kind::List => StructRole::L,
            Kind::ListItem => StructRole::LI,
            Kind::ListBody => StructRole::LBody,
            Kind::Table => StructRole::Table,
            Kind::Row => StructRole::TR,
            Kind::HeaderCell => StructRole::TH,
            Kind::Cell => StructRole::TD,
            Kind::Figure(_) => StructRole::Figure,
        };
        let mut s = self.pdf.struct_element(id);
        s.kind(role).parent(parent);
        if let Kind::Figure(Some(alt)) = &e.kind {
            s.alt(TextStr(alt));
        }
        if e.kind == Kind::HeaderCell {
            s.attributes().push().table().scope(pdf_writer::types::TableHeaderScope::Column);
        }
        {
            let mut children = s.children();
            for &(page, mcid) in &marked {
                children.marked_content_ref().page(pages[page]).marked_content_id(mcid);
            }
            for &k in &kids {
                children.struct_element(k);
            }
        }
        s.finish();
        id
    }

    /// Writes the bookmarks: each heading under the last one of a lower level before it.
    fn outline(&mut self, headings: &[Heading], pages: &[Page], page_refs: &[Ref]) -> Option<Ref> {
        // (level, title, page, y) of each heading drawn.
        let items: Vec<(u8, &str, usize, f64)> =
            headings.iter().filter_map(|h| self.firsts.get(&h.content).map(|&(p, at)| (h.level, h.title.as_str(), p, at.y))).collect();
        if items.is_empty() {
            return None;
        }
        let root = self.alloc();
        let refs: Vec<Ref> = items.iter().map(|_| self.alloc()).collect();
        // Each item's parent (an index, or the root), and its parent's kids in order.
        let mut parent: Vec<Option<usize>> = vec![None; items.len()];
        let mut open: Vec<usize> = Vec::new();
        for (k, &(level, ..)) in items.iter().enumerate() {
            while open.last().is_some_and(|&o| items[o].0 >= level) {
                open.pop();
            }
            parent[k] = open.last().copied();
            open.push(k);
        }
        let kids_of = |p: Option<usize>| -> Vec<usize> { (0..items.len()).filter(|&k| parent[k] == p).collect() };
        let count = |p: Option<usize>| -> i32 {
            fn all(parent: &[Option<usize>], p: Option<usize>) -> i32 {
                (0..parent.len()).filter(|&k| parent[k] == p).map(|k| 1 + all(parent, Some(k))).sum()
            }
            all(&parent, p)
        };
        let top = kids_of(None);
        let mut o = self.pdf.outline(root);
        o.first(refs[top[0]]).last(refs[*top.last().expect("an item")]).count(count(None));
        o.finish();
        for (k, &(_, title, page, y)) in items.iter().enumerate() {
            let siblings = kids_of(parent[k]);
            let at = siblings.iter().position(|&s| s == k).expect("an item is its parent's kid");
            let kids = kids_of(Some(k));
            let mut it = self.pdf.outline_item(refs[k]);
            it.title(TextStr(title)).parent(parent[k].map_or(root, |p| refs[p]));
            if at > 0 {
                it.prev(refs[siblings[at - 1]]);
            }
            if let Some(&n) = siblings.get(at + 1) {
                it.next(refs[n]);
            }
            if let (Some(&f), Some(&l)) = (kids.first(), kids.last()) {
                it.first(refs[f]).last(refs[l]).count(-count(Some(k)));
            }
            let top = (pages[page].height - y) as f32;
            it.dest().page(page_refs[page]).xyz(0.0, top, None);
            it.finish();
        }
        Some(root)
    }

    /// The catalog, document information and, for PDF/A, the XMP metadata and output intent.
    fn document(&mut self, catalog: Ref, tree: Ref, struct_root: Ref, outline: Option<Ref>, info: &Info) {
        let date = info.created.map(|d| {
            pdf_writer::Date::new(d.year)
                .month(d.month)
                .day(d.day)
                .hour(d.hour)
                .minute(d.minute)
                .second(d.second)
                .utc_offset_hour(0)
                .utc_offset_minute(0)
        });
        let info_ref = self.alloc();
        let mut di = self.pdf.document_info(info_ref);
        di.title(TextStr(&info.title)).creator(TextStr(&info.creator)).producer(TextStr(&info.producer));
        let keywords = info.keywords.join(", ");
        if !keywords.is_empty() {
            di.keywords(TextStr(&keywords));
        }
        if let Some(d) = date {
            di.creation_date(d).modified_date(d);
        }
        di.finish();
        let (metadata, icc) = (self.alloc(), self.alloc());
        if info.archival {
            let xmp = xmp(info, &keywords);
            self.pdf.metadata(metadata, xmp.as_bytes());
            let profile = srgb_profile();
            self.pdf.icc_profile(icc, &profile).n(3).range([0.0, 1.0, 0.0, 1.0, 0.0, 1.0]);
        }
        let mut c = self.pdf.catalog(catalog);
        c.pages(tree);
        c.pair(Name(b"StructTreeRoot"), struct_root);
        c.mark_info().marked(true);
        c.viewer_preferences().display_doc_title(true);
        if let Some(lang) = &info.language {
            c.lang(TextStr(lang));
        }
        if let Some(o) = outline {
            c.outlines(o);
        }
        if info.archival {
            c.metadata(metadata);
            c.output_intents()
                .push()
                .subtype(OutputIntentSubtype::PDFA)
                .output_condition_identifier(TextStr("sRGB IEC61966-2.1"))
                .dest_output_profile(icc);
        }
        c.finish();
        // The file's id: a hash of what identifies the document (it has no randomness of its own).
        let mut h = std::hash::DefaultHasher::new();
        (&info.title, info.created.map(|d| (d.year, d.month, d.day, d.hour, d.minute, d.second)), self.marks.len()).hash(&mut h);
        let id = Hasher::finish(&h).to_be_bytes().repeat(2);
        self.pdf.set_file_id((id.clone(), id));
    }
}

/// Writes a canvas's resources into a resource dictionary.
fn resources(mut r: pdf_writer::writers::Resources<'_>, c: &Canvas) {
    let pairs = |m: &BTreeMap<String, Ref>| m.iter().map(|(n, r)| (n.clone(), *r)).collect::<Vec<_>>();
    if !c.fonts.is_empty() {
        r.fonts().pairs(pairs(&c.fonts).iter().map(|(n, r)| (Name(n.as_bytes()), *r)));
    }
    if !c.xobjects.is_empty() {
        r.x_objects().pairs(pairs(&c.xobjects).iter().map(|(n, r)| (Name(n.as_bytes()), *r)));
    }
    if !c.states.is_empty() {
        r.ext_g_states().pairs(pairs(&c.states).iter().map(|(n, r)| (Name(n.as_bytes()), *r)));
    }
    if !c.shadings.is_empty() {
        r.shadings().pairs(pairs(&c.shadings).iter().map(|(n, r)| (Name(n.as_bytes()), *r)));
    }
    if !c.patterns.is_empty() {
        r.patterns().pairs(pairs(&c.patterns).iter().map(|(n, r)| (Name(n.as_bytes()), *r)));
    }
}

/// What a gradient brush is, for its shading.
#[derive(Clone, Copy)]
struct Gradient {
    kind: GradientKind,
    stops: crate::scene::Span,
    extend: Extend,
    transform: Transform,
}

#[derive(Clone, Copy)]
enum GradientKind {
    Linear { start: Point, end: Point },
    Radial { start: Point, r0: f32, end: Point, r1: f32 },
    Sweep { center: Point, start: f32, end: f32 },
}

/// What a shading paints: the gradient's colours, its alpha as grey (for its soft mask), or black.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Channel {
    Colour,
    Alpha,
    Black,
}

fn gradient_of(_: &Lists, brush: BrushRec) -> Gradient {
    match brush {
        BrushRec::Linear { start, end, stops, extend, transform } => Gradient { kind: GradientKind::Linear { start, end }, stops, extend, transform },
        BrushRec::Radial { start, start_radius, end, end_radius, stops, extend, transform } => {
            Gradient { kind: GradientKind::Radial { start, r0: start_radius, end, r1: end_radius }, stops, extend, transform }
        }
        BrushRec::Sweep { center, start_angle, end_angle, stops, extend, transform } => {
            Gradient { kind: GradientKind::Sweep { center, start: start_angle, end: end_angle }, stops, extend, transform }
        }
        BrushRec::Solid(_) | BrushRec::Pattern { .. } => unreachable!("only gradients have shadings"),
    }
}

/// A PostScript function from a point (x, y) to the sweep's colour there: the angle about `center` (degrees clockwise,
/// y down) from `start` to `end` as t, extended, then the stops.
fn sweep_function(stops: &[([f32; 3], f32)], center: Point, start: f32, end: f32, extend: Extend) -> String {
    let span = if end == start { 1.0 } else { end - start };
    let mut code = format!("{{ {} sub exch {} sub atan {} sub {} div ", center.y, center.x, start, span);
    code.push_str(match extend {
        Extend::Clamp => "dup 0 lt { pop 0 } if dup 1 gt { pop 1 } if ",
        Extend::Repeat => "dup floor sub ",
        Extend::Mirror => "abs dup 2 div floor 2 mul sub dup 1 gt { 2 exch sub } if ",
    });
    let colour = |c: [f32; 3]| format!("pop {} {} {} ", c[0], c[1], c[2]);
    let Some(&(first, f0)) = stops.first() else { return code + &colour([0.0; 3]) + "}" };
    // Below the first stop, its colour; between stops, linear; past the last, its colour.
    let mut body = colour(stops.last().map_or(first, |s| s.0));
    for w in stops.windows(2).rev() {
        let ((c0, a), (c1, b)) = (w[0], w[1]);
        let seg = if b > a {
            let d = [c1[0] - c0[0], c1[1] - c0[1], c1[2] - c0[2]];
            format!("{a} sub {} div dup {} mul {} add exch dup {} mul {} add exch {} mul {} add ", b - a, d[0], c0[0], d[1], c0[1], d[2], c0[2])
        } else {
            colour(c1)
        };
        body = format!("dup {b} le {{ {seg}}} {{ {body}}} ifelse ");
    }
    let body = format!("dup {f0} lt {{ {}}} {{ {body}}} ifelse ", colour(first));
    code + &body + "}"
}

/// Writes a path's verbs and points as path operators.
fn path_ops(c: &mut Content, verbs: &[Verb], points: &[Point]) {
    let mut p = points.iter();
    let (mut at, mut start) = (Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: 0.0 });
    for v in verbs {
        match v {
            Verb::Move => {
                if let Some(&a) = p.next() {
                    c.move_to(a.x, a.y);
                    (at, start) = (a, a);
                }
            }
            Verb::Line => {
                if let Some(&a) = p.next() {
                    c.line_to(a.x, a.y);
                    at = a;
                }
            }
            Verb::Quad => {
                // A quadratic as the cubic it is: PDF has no quadratic operator.
                if let (Some(&q), Some(&a)) = (p.next(), p.next()) {
                    let k = 2.0 / 3.0;
                    c.cubic_to(at.x + k * (q.x - at.x), at.y + k * (q.y - at.y), a.x + k * (q.x - a.x), a.y + k * (q.y - a.y), a.x, a.y);
                    at = a;
                }
            }
            Verb::Cubic => {
                if let (Some(&c0), Some(&c1), Some(&a)) = (p.next(), p.next(), p.next()) {
                    c.cubic_to(c0.x, c0.y, c1.x, c1.y, a.x, a.y);
                    at = a;
                }
            }
            Verb::Close => {
                c.close_path();
                at = start;
            }
        }
    }
}

/// A kurbo path as verbs and points.
fn path_lists(path: &BezPath) -> (Vec<Verb>, Vec<Point>) {
    let (mut verbs, mut points) = (Vec::new(), Vec::new());
    let p = |q: kurbo::Point| Point { x: q.x as f32, y: q.y as f32 };
    for el in path.iter() {
        match el {
            PathEl::MoveTo(a) => {
                verbs.push(Verb::Move);
                points.push(p(a));
            }
            PathEl::LineTo(a) => {
                verbs.push(Verb::Line);
                points.push(p(a));
            }
            PathEl::QuadTo(c, a) => {
                verbs.push(Verb::Quad);
                points.extend([p(c), p(a)]);
            }
            PathEl::CurveTo(c0, c1, a) => {
                verbs.push(Verb::Cubic);
                points.extend([p(c0), p(c1), p(a)]);
            }
            PathEl::ClosePath => verbs.push(Verb::Close),
        }
    }
    (verbs, points)
}

fn bounds_of(points: &[Point]) -> kurbo::Rect {
    points
        .iter()
        .fold(None, |b: Option<kurbo::Rect>, p| {
            let q = kurbo::Point::new(f64::from(p.x), f64::from(p.y));
            Some(b.map_or(kurbo::Rect::from_points(q, q), |b| b.union_pt(q)))
        })
        .unwrap_or(kurbo::Rect::ZERO)
}

fn rect_ops(c: &mut Content, r: kurbo::Rect) {
    let r = r.abs();
    c.rect(r.x0 as f32, r.y0 as f32, r.width() as f32, r.height() as f32);
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
        Cmd::Glyphs { paint, .. }
        | Cmd::Rect { paint, .. }
        | Cmd::RRect { paint, .. }
        | Cmd::Path { paint, .. }
        | Cmd::EvenOddPath { paint, .. }
        | Cmd::Frame { paint, .. } => op.lists.paints[paint as usize].blend,
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
fn native(mode: BlendMode) -> Option<PdfBlend> {
    Some(match mode {
        BlendMode::Multiply => PdfBlend::Multiply,
        BlendMode::Screen => PdfBlend::Screen,
        BlendMode::Overlay => PdfBlend::Overlay,
        BlendMode::Darken => PdfBlend::Darken,
        BlendMode::Lighten => PdfBlend::Lighten,
        BlendMode::ColorDodge => PdfBlend::ColorDodge,
        BlendMode::ColorBurn => PdfBlend::ColorBurn,
        BlendMode::HardLight => PdfBlend::HardLight,
        BlendMode::SoftLight => PdfBlend::SoftLight,
        BlendMode::Difference => PdfBlend::Difference,
        BlendMode::Exclusion => PdfBlend::Exclusion,
        BlendMode::Hue => PdfBlend::Hue,
        BlendMode::Saturation => PdfBlend::Saturation,
        BlendMode::Color => PdfBlend::Color,
        BlendMode::Luminosity => PdfBlend::Luminosity,
        _ => return None,
    })
}

fn affine(t: Transform) -> Affine {
    Affine::new([t.a, t.b, t.c, t.d, t.e, t.f].map(f64::from))
}

fn coeffs(t: Affine) -> [f32; 6] {
    t.as_coeffs().map(|v| v as f32)
}

fn unpremultiply(c: PremulColor) -> Color {
    if c.a == 0 {
        return Color::TRANSPARENT;
    }
    let un = |v: u8| ((u32::from(v) * 255 + u32::from(c.a) / 2) / u32::from(c.a)).min(255) as u8;
    Color::rgba(un(c.r), un(c.g), un(c.b), c.a)
}

fn deflate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
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

/// A JPEG's colour components (1 grey, 3 colour) from its frame header; `None` for 4 (CMYK, which PDF inverts by the
/// Adobe marker's whim: decoded instead) or a file without one.
fn jpeg_components(data: &[u8]) -> Option<u8> {
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        let len = usize::from(u16::from_be_bytes([data[i + 2], data[i + 3]]));
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            return data.get(i + 9).copied().filter(|&n| n == 1 || n == 3);
        }
        i += 2 + len;
    }
    None
}

/// A hash of colour filters, the ink and the interpolation; 0 for none, the paint and interpolated.
fn filter_hash(filters: &[ColorFilter], ink: Ink, interpolate: bool) -> u64 {
    if filters.is_empty() && ink == Ink::Paint && interpolate {
        return 0;
    }
    let mut h = std::hash::DefaultHasher::new();
    (ink, interpolate).hash(&mut h);
    for f in filters {
        match f {
            ColorFilter::Matrix(m) => m.iter().for_each(|v| v.to_bits().hash(&mut h)),
            ColorFilter::Table(t) => t.hash(&mut h),
        }
    }
    Hasher::finish(&h) | 1
}

/// A subset font's name: six letters from the glyphs used, then the face's PostScript name.
fn base_name(f: &skrifa::FontRef<'_>, font: FontId, used: &std::collections::BTreeSet<u16>) -> String {
    let ps: String = f
        .localized_strings(skrifa::string::StringId::POSTSCRIPT_NAME)
        .english_or_first()
        .map(|s| s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect())
        .filter(|s: &String| !s.is_empty())
        .unwrap_or_else(|| format!("Font{}", font.0));
    let mut h = std::hash::DefaultHasher::new();
    used.hash(&mut h);
    let mut v = Hasher::finish(&h);
    let tag: String = (0..6)
        .map(|_| {
            let c = (b'A' + (v % 26) as u8) as char;
            v /= 26;
            c
        })
        .collect();
    format!("{tag}+{ps}")
}

/// Text for XML.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The XMP metadata PDF/A-2b asks for, matching the document information.
fn xmp(info: &Info, keywords: &str) -> String {
    let date = info
        .created
        .map(|d| format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00", d.year, d.month, d.day, d.hour, d.minute, d.second))
        .unwrap_or_default();
    let mut h = std::hash::DefaultHasher::new();
    (&info.title, &date).hash(&mut h);
    let id = format!("{:016x}{:016x}", Hasher::finish(&h), Hasher::finish(&h).rotate_left(17));
    let uuid = format!("uuid:{}-{}-{}-{}-{}", &id[0..8], &id[8..12], &id[12..16], &id[16..20], &id[20..32]);
    let mut x = String::from(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" xmlns:xmpMM=\"http://ns.adobe.com/xap/1.0/mm/\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">\n",
    );
    let _ = writeln!(x, "<dc:format>application/pdf</dc:format>");
    let _ = writeln!(x, "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>", escape(&info.title));
    if let Some(lang) = &info.language {
        let _ = writeln!(x, "<dc:language><rdf:Bag><rdf:li>{}</rdf:li></rdf:Bag></dc:language>", escape(lang));
    }
    let _ = writeln!(x, "<xmp:CreatorTool>{}</xmp:CreatorTool>", escape(&info.creator));
    if !date.is_empty() {
        let _ = writeln!(x, "<xmp:CreateDate>{date}</xmp:CreateDate>\n<xmp:ModifyDate>{date}</xmp:ModifyDate>");
    }
    let _ = writeln!(x, "<pdf:Producer>{}</pdf:Producer>", escape(&info.producer));
    if !keywords.is_empty() {
        let _ = writeln!(x, "<pdf:Keywords>{}</pdf:Keywords>", escape(keywords));
    }
    let _ = writeln!(x, "<xmpMM:DocumentID>{uuid}</xmpMM:DocumentID>\n<xmpMM:InstanceID>{uuid}</xmpMM:InstanceID>");
    x.push_str("<pdfaid:part>2</pdfaid:part>\n<pdfaid:conformance>B</pdfaid:conformance>\n</rdf:Description>\n</rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>");
    x
}

/// An sRGB ICC profile (version 2, display class): sRGB's primaries adapted to D50, as s15.16 XYZ, and its transfer
/// curve as a 1024-entry table. For the PDF/A output intent.
fn srgb_profile() -> Vec<u8> {
    fn s15(v: f64) -> [u8; 4] {
        ((v * 65536.0).round() as i32).to_be_bytes()
    }
    fn xyz(v: [f64; 3]) -> Vec<u8> {
        let mut t = b"XYZ \0\0\0\0".to_vec();
        for c in v {
            t.extend(s15(c));
        }
        t
    }
    let curve = {
        let mut t = b"curv\0\0\0\0".to_vec();
        t.extend(1024u32.to_be_bytes());
        for i in 0..1024 {
            let v = f64::from(i) / 1023.0;
            let lin = if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
            t.extend(((lin * 65535.0).round() as u16).to_be_bytes());
        }
        t
    };
    let text = |s: &str| {
        let mut t = b"text\0\0\0\0".to_vec();
        t.extend(s.as_bytes());
        t.push(0);
        t
    };
    let desc = {
        let s = b"sRGB IEC61966-2.1";
        let mut t = b"desc\0\0\0\0".to_vec();
        t.extend((s.len() as u32 + 1).to_be_bytes());
        t.extend(s);
        t.push(0);
        // No Unicode or ScriptCode description.
        t.extend([0u8; 4 + 4 + 2 + 1 + 67]);
        t
    };
    let tags: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"desc", desc),
        (*b"cprt", text("No copyright: sRGB's published primaries and curve")),
        (*b"wtpt", xyz([0.9642, 1.0, 0.8249])),
        (*b"rXYZ", xyz([0.4361, 0.2225, 0.0139])),
        (*b"gXYZ", xyz([0.3851, 0.7169, 0.0971])),
        (*b"bXYZ", xyz([0.1431, 0.0606, 0.7141])),
        (*b"rTRC", curve.clone()),
        (*b"gTRC", curve.clone()),
        (*b"bTRC", curve),
    ];
    let mut body = Vec::new();
    let mut table = Vec::new();
    let start = 128 + 4 + 12 * tags.len();
    for (sig, data) in &tags {
        while !(start + body.len()).is_multiple_of(4) {
            body.push(0);
        }
        table.extend(sig);
        table.extend(((start + body.len()) as u32).to_be_bytes());
        table.extend((data.len() as u32).to_be_bytes());
        body.extend(data);
    }
    let size = start + body.len();
    let mut p = Vec::with_capacity(size);
    p.extend((size as u32).to_be_bytes());
    p.extend([0u8; 4]); // CMM
    p.extend(0x0210_0000u32.to_be_bytes());
    p.extend(b"mntrRGB XYZ ");
    p.extend([0u8; 12]); // date
    p.extend(b"acsp");
    p.extend([0u8; 4 + 4 + 4 + 4 + 8 + 4]); // platform, flags, manufacturer, model, attributes, intent
    p.extend(s15(0.9642));
    p.extend(s15(1.0));
    p.extend(s15(0.8249));
    p.extend([0u8; 4 + 16 + 28]); // creator, id, reserved
    p.extend((tags.len() as u32).to_be_bytes());
    p.extend(table);
    p.extend(body);
    p
}
