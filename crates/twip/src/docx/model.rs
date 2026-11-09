//! The layout engine's input: a document with every style resolved (the STYLE rules). Lengths are points.

use super::font_table::GenericFamily;
use std::collections::HashMap;
use std::rc::Rc;

use super::props::{Border, FramePr, Jc, Leader, LineRule, Look, Tab, TabKind, TablePos, VertAlign, Width};
use super::review::{
    CellRevisions, CommentId, Comments, GridChange, IdCounter, ParaChange, People, Protection, RangeMark, ReviewView, RowRevisions, RunRevisions,
    SectionChange, TableChange,
};

/// Effective character formatting of a run.
#[derive(Clone, Debug, PartialEq)]
pub struct RunStyle {
    /// Family names per script slot, before font substitution.
    pub ascii: String,
    pub h_ansi: String,
    pub east_asia: String,
    pub cs: String,
    pub east_asia_unset: bool,
    pub east_asia_named: bool,
    pub hint_east_asia: bool,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub caps: bool,
    pub small_caps: bool,
    pub strike: bool,
    pub dstrike: bool,
    pub hidden: bool,
    pub underline: Option<String>,
    pub color: Option<u32>,
    pub highlight: Option<u32>,
    pub shading: Option<u32>,
    /// The shading's pattern, pattern color and fill, as written.
    pub shd: Option<super::props::Shd>,
    /// The text's border (`w:bdr`): a box around runs in a row with the same one.
    pub border: Option<super::props::Border>,
    pub vert_align: VertAlign,
    /// Raise (+) / lower (−), points.
    pub position: f64,
    /// Extra character spacing, points.
    pub spacing: f64,
    /// Horizontal scale, percent.
    pub scale: f64,
    /// Kern at or above this size; 0 = off.
    pub kern: f64,
    pub ligatures: u8,
    /// `w:em`, when it marks the text ("dot", "comma", "circle", "underDot").
    pub em: Option<String>,
    /// In a table of contents' result: the Hyperlink character style doesn't show ([`crate::docx::resolve::Resolver::rerun`]).
    pub toc: bool,
    pub lang: String,
    pub style_id: Option<String>,
    /// The `w:rPr` as written (editable reads only): what saving writes back for new runs in this formatting.
    pub direct: Raw,
    /// Tracked insertion, deletion and formatting change of the run (or, on a paragraph's mark, of the mark).
    pub rev: RunRevisions,
    /// The generic categories the document's font table gives the families of the `ascii`, `h_ansi` and `east_asia`
    /// slots.
    pub categories: [Option<GenericFamily>; 3],
}

impl RunStyle {
    /// The generic category the document's font table gives `family`, when it is one of this run's slots.
    pub fn category(&self, family: &str) -> Option<GenericFamily> {
        [&self.ascii, &self.h_ansi, &self.east_asia].iter().position(|f| f.as_str() == family).and_then(|k| self.categories[k])
    }
}

pub type Run = Rc<RunStyle>;

/// Source XML carried for saving (a run's `w:rPr`, a paragraph's `w:pPr`). Never part of equality: formatting
/// compares by its effect.
#[derive(Clone, Debug, Default)]
pub struct Raw(pub Option<Rc<str>>);

impl PartialEq for Raw {
    fn eq(&self, _: &Raw) -> bool {
        true
    }
}

/// A table row's HTML division.
#[derive(Clone, Debug, PartialEq)]
pub struct RowDiv {
    pub id: i64,
    pub shift: f64,
    pub lead: f64,
    pub trail: f64,
    pub boxes: Rc<[DivBox]>,
}

/// A bordered HTML division's box, as one paragraph in it sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct DivBox {
    /// Top, left, bottom, right.
    pub borders: [Option<Border>; 4],
    /// The division's left and right edges, from the text column's.
    pub left: f64,
    pub right: f64,
    /// The box opens above this paragraph, closes below it.
    pub starts: bool,
    pub ends: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParaStyle {
    pub style_id: Option<String>,
    pub jc: Jc,
    pub bidi: bool,
    pub ind_left: f64,
    pub ind_right: f64,
    /// + first line, − hanging.
    pub ind_first: f64,
    pub space_before: f64,
    pub space_after: f64,
    pub before_auto: bool,
    pub after_auto: bool,
    /// Auto: multiple (1.0 = single); exact / at least: points.
    pub line: f64,
    pub line_rule: LineRule,
    pub contextual: bool,
    pub snap_to_grid: bool,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub page_break_before: bool,
    pub widow_control: bool,
    pub suppress_auto_hyphens: bool,
    /// `w:textboxTightWrap` in a text box: 0 none, 1 all lines, 2 first and last, 3 first only, 4 last only.
    pub tight_wrap: u8,
    /// `w:suppressOverlap`: the paragraph's frame may not overlap other frames and floats.
    pub suppress_overlap: bool,
    pub suppress_line_numbers: bool,
    /// `w:mirrorIndents`: on mirrored pages the indents swap sides.
    pub mirror_indents: bool,
    /// The HTML division the paragraph is in (`w:divId`).
    pub div: Option<i64>,
    /// The bordered divisions around the paragraph, outermost first.
    pub div_boxes: Rc<[DivBox]>,
    pub tabs: Vec<Tab>,
    pub border_top: Option<Border>,
    pub border_bottom: Option<Border>,
    pub border_left: Option<Border>,
    pub border_right: Option<Border>,
    pub border_between: Option<Border>,
    pub shading: Option<u32>,
    /// The shading's pattern, pattern color and fill, as written.
    pub shd: Option<super::props::Shd>,
    pub frame: Option<FramePr>,
    pub text_alignment: String,
    pub outline_lvl: Option<i64>,
    pub list_jc: String,
    /// The list the paragraph is in (numId, ilvl; direct or from its style), and that level's `w:numFmt`.
    pub num: Option<(i64, i64)>,
    pub list_fmt: Option<String>,
    pub list: Option<i64>,
    pub mark_size: Option<f64>,
    /// The `w:pPr` as written (editable reads only), what formatting edits change and saving writes back.
    pub direct: Raw,
    /// A tracked change of the paragraph's properties.
    pub change: Option<Rc<ParaChange>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakKind {
    Line,
    Page,
    Column,
    /// Text-wrapping breaks (`w:br w:clear`).
    ClearLeft,
    ClearRight,
    ClearAll,
}

impl BreakKind {
    pub fn is_line(self) -> bool {
        matches!(self, BreakKind::Line | BreakKind::ClearLeft | BreakKind::ClearRight | BreakKind::ClearAll)
    }

    /// The break `w:br@type` and `@clear` give.
    pub fn of(kind: Option<&str>, clear: Option<&str>) -> BreakKind {
        match (kind, clear) {
            (Some("page"), _) => BreakKind::Page,
            (Some("column"), _) => BreakKind::Column,
            (_, Some("left")) => BreakKind::ClearLeft,
            (_, Some("right")) => BreakKind::ClearRight,
            (_, Some("all")) => BreakKind::ClearAll,
            _ => BreakKind::Line,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoteKind {
    Foot,
    End,
}

/// A field's identity: its begin, separator and end markers share it. Process-local, kept
/// through edits, undo and redo; new on every read and paste.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FieldId(pub u64);

impl FieldId {
    /// A new id (unique in the process).
    pub fn next() -> FieldId {
        FieldId(next_para_id())
    }
}

/// A field or content control: what the host names (`f12` / `c7` in JSON).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ElementId {
    Field(FieldId),
    Control(ControlId),
}

/// Whether a field shows its result or its code (Alt+F9, Shift+F9; not saved).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FieldView {
    #[default]
    Result,
    Code,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldKind {
    Page,
    NumPages,
    SectionPages,
    Section,
    PageRef(String),
    Ref(String),
    Seq(String),
    StyleRef(String),
    Date,
    If,
    Bookmark(String),
    /// A code whose first word can be neither a field name nor a bookmark (`"http://…"` left by a merge): Word shows
    /// nothing.
    Invalid,
    SubTemplate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub w: f64,
    pub h: f64,
    /// Index into [`Document::images`].
    pub image: Option<usize>,
    /// Its alternative text (`wp:docPr/@descr`).
    pub alt: Rc<str>,
    /// Lock aspect ratio (`a:graphicFrameLocks/@noChangeAspect` or `a:picLocks/@noChangeAspect`): corner handles keep
    /// the ratio.
    pub lock_aspect: bool,
    pub effect: [f64; 4],
    /// The DrawingML object: its geometry, paint and transform (`None`: a picture inserted without them).
    pub shape: Option<Box<crate::drawing::Shape>>,
    /// An inline shape's text (`wps:txbx`): its story, edited as a text box's (`StoryId::TextBox` of its id).
    pub text: Option<Box<InlineText>>,
}

impl Inline {
    /// The story this inline holds and the id it goes by: a float's text box, or an inline shape's text.
    pub fn story(&self) -> Option<(u64, &TextBox)> {
        match self {
            Inline::Float(f, _) => f.text_box.as_ref().map(|tb| (f.id, tb)),
            Inline::Picture(p, _) => p.text.as_ref().map(|t| (t.id, &t.text_box)),
            _ => None,
        }
    }

    pub fn story_mut(&mut self) -> Option<&mut TextBox> {
        match self {
            Inline::Float(f, _) => f.text_box.as_mut(),
            Inline::Picture(p, _) => p.text.as_mut().map(|t| &mut t.text_box),
            _ => None,
        }
    }
}

/// An inline shape's story and the id it goes by (from the floats' id space).
#[derive(Clone, Debug, PartialEq)]
pub struct InlineText {
    pub id: u64,
    pub text_box: TextBox,
}

/// An anchored (floating) picture or text box (the FLOAT rules).
#[derive(Clone, Debug, PartialEq)]
pub struct Float {
    /// Identity in layouts and editing ([`next_para_id`]'s counter): fragments name it, selections and drags keep it.
    pub id: u64,
    pub w: f64,
    pub auto_w: bool,
    pub h: f64,
    pub image: Option<usize>,
    /// A picture's alternative text (`wp:docPr/@descr`).
    pub alt: Rc<str>,
    /// Lock aspect ratio, as for [`Picture::lock_aspect`].
    pub lock_aspect: bool,
    /// Text box content, laid out at the box's content width.
    pub text_box: Option<TextBox>,
    pub h_relative: String,
    pub h_align: Option<String>,
    pub h_offset: f64,
    pub v_relative: String,
    pub v_align: Option<String>,
    pub v_offset: f64,
    /// `square`, `tight`, `through`, `topAndBottom`, `none`.
    pub wrap: String,
    /// `bothSides`, `left`, `right`, `largest`.
    pub wrap_text: String,
    pub dist: [f64; 4],
    pub effect: [f64; 4],
    /// The wrap polygon of a tight or through float, as fractions of its box.
    pub poly: Option<Rc<[(f64, f64)]>>,
    pub behind: bool,
    pub allow_overlap: bool,
    pub layout_in_cell: bool,
    /// Document order of the float, for F13.
    pub order: usize,
    pub z: u32,
    pub rot_swap: bool,
    pub relative: Relative,
    /// The source `w:drawing` element's byte range in its part: saving drops it when the float is deleted.
    pub src: Option<std::ops::Range<usize>>,
    /// `wp:anchor/@locked`: the anchor stays in its paragraph when the float is moved.
    pub locked: bool,
    /// A frame's paragraphs in the source (frames.rs): saving drops them when the frame is deleted.
    pub frame_src: Option<std::ops::Range<usize>>,
    pub parts: Vec<GroupPart>,
    /// The DrawingML object (geometry, paint, transform, group tree); `None` for frames and floats made without one.
    pub shape: Option<Box<crate::drawing::Shape>>,
}

/// Relative size and position (`wp14:sizeRelH`, `wp14:pctPosHOffset`, VML `mso-width-percent`, `mso-left-percent`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Relative {
    /// Width and height: the box they're relative to and the share in tenths of a percent.
    pub size: [Option<(String, i64)>; 2],
    /// Horizontal and vertical position, in tenths of a percent of the reference box.
    pub pos: [Option<i64>; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupPart {
    pub rect: [f64; 4],
    pub image: Option<usize>,
    pub text_box: Option<TextBox>,
    /// The part's place in the float's shape tree.
    pub path: crate::drawing::ChildPath,
}

impl Float {
    /// The picture control a floating picture control holds: the float is a text box with no fill, no line and no
    /// insets whose story is one paragraph holding only that control's one picture.
    pub fn picture_control(&self, controls: &HashMap<ControlId, Control>) -> Option<ControlId> {
        let tb = self.text_box.as_ref()?;
        if tb.insets != [0.0; 4] || tb.outline != 0.0 || tb.blocks.len() != 1 {
            return None;
        }
        if let Some(crate::drawing::ShapeKind::Sp(sp)) = self.shape.as_ref().map(|s| &s.kind) {
            match &sp.fill {
                Some(crate::drawing::paint::Fill::None) => {}
                None if sp.style.is_none() => {}
                _ => return None,
            }
        }
        let Block::Para(p) = &tb.blocks[0] else { return None };
        let picture = |c: &ControlId| controls.get(c).is_some_and(|k| k.props.kind == ControlKind::Picture);
        let content: Vec<&Inline> = p
            .inlines
            .iter()
            .filter(|i| !matches!(i, Inline::Bookmark(..) | Inline::Range(..)) && !matches!(i, Inline::Text(t, _) if t.is_empty()))
            .collect();
        match content.as_slice() {
            [Inline::ControlStart(a), Inline::Picture(..), Inline::ControlEnd(b)] if a == b && picture(a) => Some(*a),
            [Inline::Picture(..)] if p.controls.len() == 1 && picture(&p.controls[0]) => Some(p.controls[0]),
            _ => None,
        }
    }

    pub fn in_cell(&self, compat: Compat) -> bool {
        compat == Compat::Generic || self.layout_in_cell || self.h_relative == "character" || self.v_relative == "line"
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBox {
    pub blocks: Vec<Block>,
    pub insets: [f64; 4],
    pub outline: f64,
    pub auto_fit: bool,
    pub v_anchor: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String, Run),
    Tab(Run),
    /// `w:ptab`: a tab to an absolute position, not to the next stop.
    PTab(PTab, Run),
    Break(BreakKind, Run),
    /// `w:noBreakHyphen`, `w:softHyphen` are in the text as U+2011 / U+00AD; `w:sym` as its character.
    Sym {
        ch: char,
        font: String,
        run: Run,
        /// The checkbox control it is the face of (`Document::controls`), if any.
        control: Option<ControlId>,
    },
    Picture(Picture, Run),
    Float(Box<Float>, Run),
    NoteRef {
        kind: NoteKind,
        id: i64,
        custom: bool,
        run: Run,
    },
    /// `w:footnoteRef` / `w:endnoteRef` inside a note: the note's own number (`id` = the note's id).
    NoteMark(NoteKind, i64, Run),
    Separator {
        continuation: bool,
        run: Run,
    },
    /// A field's start (`w:fldChar begin`). The inlines up to its [`Inline::FieldSep`] (or its
    /// [`Inline::FieldEnd`] when it has none) are its code, those after the separator its result; a nested field is
    /// another marker triple inside either.
    FieldBegin {
        id: FieldId,
        /// The kind layout evaluates, parsed from the code ([`super::fields::parse`]); `None`: the result is shown.
        kind: Option<FieldKind>,
        /// `w:fldLock`, `w:dirty`.
        lock: bool,
        dirty: bool,
        view: FieldView,
        /// Template design: what an empty result shows; not saved.
        label: Option<Rc<str>>,
        value: Option<String>,
        run: Run,
    },
    /// `w:fldChar separate`: the end of a field's code, the start of its result.
    FieldSep {
        id: FieldId,
        run: Run,
    },
    /// `w:fldChar end`.
    FieldEnd {
        id: FieldId,
        run: Run,
    },
    /// Start of a bookmark (for PAGEREF/REF).
    Bookmark(String),
    /// Where a comment's or a move's range starts or ends. No units.
    Range(RangeMark),
    /// A comment's reference mark (`w:commentReference`): where the comment is anchored. No units.
    CommentRef {
        id: CommentId,
        run: Run,
    },
    /// Where an inline content control's content starts and ends. No units: a caret beside one is
    /// beside the control's edge.
    ControlStart(ControlId),
    ControlEnd(ControlId),
    /// A ruby (`w:ruby`): one unit.
    Ruby(Rc<Ruby>, Run),
}

/// A ruby: base text with its annotation set above it, laid out as one object.
#[derive(Clone, Debug, PartialEq)]
pub struct Ruby {
    pub base: Vec<(String, Run)>,
    pub text: Vec<(String, Run)>,
    /// `w:rubyAlign`: center, distributeLetter, distributeSpace, left, right, rightVertical (0 to 5).
    pub align: u8,
    /// `w:hpsRaise`: the annotation's baseline above the base's, points.
    pub raise: f64,
    /// The `w:ruby` element as read: what saving writes.
    pub xml: Raw,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListLabel {
    pub text: String,
    pub run: Run,
    pub jc: String,
    pub suff: String,
    /// A picture bullet's image (an index into [`Document::images`]), drawn instead of the text.
    pub picture: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    pub style: Rc<ParaStyle>,
    pub mark: Run,
    pub list: Option<ListLabel>,
    pub inlines: Vec<Inline>,
    /// Where the paragraph was read from (editable reads of the main body only; new paragraphs have none).
    pub src: Option<Rc<ParaSrc>>,
    /// Identity across edits and layouts ([`next_para_id`]); a clone keeps it, so a copied or split-off paragraph
    /// needs a fresh one. Layout tags drawn glyphs with it, and incremental layout reuses pages by it.
    pub id: u64,
    /// The block-level content controls around it, outermost first: consecutive blocks with an id are
    /// that control's content.
    pub controls: Vec<ControlId>,
}

/// A new paragraph id (unique in the process).
pub fn next_para_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A body paragraph's place in the main part's XML ([`Source::xml`] byte offsets), for lossless saving.
#[derive(Debug, PartialEq)]
pub struct ParaSrc {
    pub ranges: Vec<std::ops::Range<usize>>,
    /// The elements that produced its units, in order, with how many units each did: `w:t`, `w:tab`, a drawing, a
    /// whole complex field (from its `begin` run to its `end` run), ...
    pub segs: Vec<(std::ops::Range<usize>, usize)>,
    /// The elements inside each simple field and hyperlink seg (by its start) that produced its result's units.
    pub simple: Vec<(usize, Vec<(std::ops::Range<usize>, usize)>)>,
    /// Content that can't be mapped back (a field whose runs don't share a parent): edits rewrite it from the model.
    pub lossy: bool,
}

/// The package an editable document was read from ([`super::read_editable`]).
pub struct Source {
    pub package: Vec<u8>,
    /// The main part's name and XML as read.
    pub main: String,
    pub xml: String,
    /// Each section's blocks as read (what saving compares the edited document with).
    pub blocks: Vec<Vec<Block>>,
    /// Each header's and footer's part (by name, as [`Document::stories`]): its XML and its blocks as read.
    pub stories: HashMap<String, (String, Vec<Block>)>,
    /// Each read image's part name, by index in [`Document::images`] (what a filled placeholder's image replaces).
    pub image_parts: Vec<String>,
    /// The footnotes and endnotes parts: name, XML, and whether it holds footnotes.
    pub notes: Vec<(String, String, bool)>,
    /// Each note's blocks as read, by id.
    pub footnotes: HashMap<i64, Vec<Block>>,
    pub endnotes: HashMap<i64, Vec<Block>>,
    /// The content controls as read: saving takes the markup of those no longer in the document out.
    pub controls: HashMap<ControlId, Control>,
    /// The glossary part as read, when the package has one.
    pub glossary: Option<GlossarySource>,
}

/// The glossary part as read: its name and XML, and each building block's `w:docPart` byte range in that XML with
/// its body as read (what saving compares the edited body with), in glossary order.
#[derive(Debug, PartialEq)]
pub struct GlossarySource {
    pub part: String,
    pub xml: String,
    pub parts: Vec<(std::ops::Range<usize>, Vec<Block>)>,
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Source({}, {} bytes)", self.main, self.xml.len())
    }
}

/// A custom XML part: opaque to the editor, kept with the document.
#[derive(Clone, Debug, PartialEq)]
pub struct DataPart {
    /// Its root element's namespace: how the host names it (Word's `CustomXMLParts.SelectByNamespace`).
    pub namespace: Option<String>,
    /// `ds:itemID`, unique in the package.
    pub item_id: String,
    pub xml: String,
    /// Its item part's name in the package as read (`customXml/item1.xml`); `None` for a new part.
    pub src: Option<String>,
    /// Written on save (new, or its XML changed); an unchanged part's bytes are copied.
    pub changed: bool,
}

/// The glossary document part: the package's building blocks, and how their bodies' style and numbering ids map
/// onto the document's tables.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Glossary {
    /// In glossary order; a building block's index here is its identity for the session (removed ones stay, marked).
    pub parts: Vec<BuildingBlock>,
    pub map: super::glossary::GlossaryMap,
}

impl Glossary {
    /// The live building block named `name` in `category` (and `gallery`, when given): the first, when several are.
    pub fn find(&self, name: &str, category: &str, gallery: Option<&Gallery>) -> Option<usize> {
        self.parts
            .iter()
            .position(|b| b.state != PartState::Removed && b.name == name && b.category == category && gallery.is_none_or(|g| *g == b.gallery))
    }
}

/// One building block (`w:docPart`).
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingBlock {
    /// `w:docPartPr/w:name`.
    pub name: String,
    /// `w:category/w:name` and `w:category/w:gallery`.
    pub category: String,
    pub gallery: Gallery,
    /// `w:guid`, as written.
    pub guid: Option<String>,
    /// `w:docPartBody`.
    pub body: Vec<Block>,
    pub state: PartState,
    /// False for a body holding a `w:sectPr`: it is kept as written and never offered for editing.
    pub editable: bool,
}

/// A building block's gallery (`w:gallery`): the ones the editor names, and any other value as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gallery {
    /// `placeholder`: content control placeholder text.
    Placeholder,
    /// `autoTxt`.
    AutoText,
    /// `custom1` to `custom5`.
    Custom(u8),
    Other(String),
}

impl Gallery {
    pub fn parse(v: &str) -> Gallery {
        match v {
            "placeholder" => Gallery::Placeholder,
            "autoTxt" => Gallery::AutoText,
            _ => match v.strip_prefix("custom").and_then(|n| n.parse::<u8>().ok()) {
                Some(n @ 1..=5) => Gallery::Custom(n),
                _ => Gallery::Other(v.to_string()),
            },
        }
    }

    /// The `w:gallery` value.
    pub fn xml(&self) -> String {
        match self {
            Gallery::Placeholder => "placeholder".into(),
            Gallery::AutoText => "autoTxt".into(),
            Gallery::Custom(n) => format!("custom{n}"),
            Gallery::Other(v) => v.clone(),
        }
    }
}

/// Whether a building block is as read, added since, or removed since (kept so undo can bring it back).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PartState {
    #[default]
    AsRead,
    Added,
    Removed,
}

/// A content control's or custom XML wrapper's identity: process-local, kept through edits,
/// undo and redo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ControlId(pub u64);

impl ControlId {
    /// A new id (unique in the process).
    pub fn next() -> ControlId {
        ControlId(next_para_id())
    }
}

/// A content control (`w:sdt`) or custom XML wrapper: its properties, and the markup it was read
/// with (written back as it was while its properties are unchanged).
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub props: ControlProps,
    /// Its start tag with its properties (`w:sdtPr`, `w:sdtEndPr`) and the content's start tag, as read; empty for a
    /// new control.
    pub open: String,
    /// The content's and the control's end tags, as read.
    pub close: String,
    /// Its properties as read: while `props` equals them, `open` is what is written.
    pub read: Option<ControlProps>,
    /// Where `open` and `close` start in the part it was read from (the writer checks the bytes there before using
    /// them).
    pub at: Option<(usize, usize)>,
}

impl Control {
    /// A new control (no markup read).
    pub fn new(props: ControlProps) -> Control {
        Control { props, open: String::new(), close: String::new(), read: None, at: None }
    }

    /// Whether it is written with the markup it was read with.
    pub fn as_read(&self) -> bool {
        !self.open.is_empty() && self.read.as_ref() == Some(&self.props)
    }
}

/// A content control's properties (`w:sdtPr`), parsed where they are read. The settings the
/// editor never changes are kept as raw XML, written back in schema order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ControlProps {
    pub kind: ControlKind,
    pub tag: Option<String>,
    /// `w:alias`: the control's title.
    pub alias: Option<String>,
    pub lock: Lock,
    /// `w:showingPlcHdr`.
    pub showing_placeholder: bool,
    /// `w:placeholder` (its docPart reference) as read.
    pub placeholder: Option<String>,
    /// `w:dataBinding` as read: kept, not evaluated.
    pub binding: Option<String>,
    /// The kind's element as read (`w:date` with its children, `w:dropDownList`, ...): written back unchanged.
    pub kind_xml: Option<String>,
    /// Every other `w:sdtPr` child (`w:rPr`, `w:id`, `w:temporary`, ...), by local name, in reading order.
    pub rest: Vec<(String, String)>,
}

/// `w:lock`: what the user may not do to a control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lock {
    #[default]
    None,
    /// `sdtLocked`: the control can't be deleted.
    ControlLocked,
    /// `contentLocked`: its content can't be edited.
    ContentLocked,
    /// `sdtContentLocked`: neither.
    Both,
}

impl Lock {
    pub fn parse(v: &str) -> Lock {
        match v {
            "sdtLocked" => Lock::ControlLocked,
            "contentLocked" => Lock::ContentLocked,
            "sdtContentLocked" => Lock::Both,
            _ => Lock::None,
        }
    }

    /// The `w:lock` value, `None` for no lock.
    pub fn val(self) -> Option<&'static str> {
        match self {
            Lock::None => None,
            Lock::ControlLocked => Some("sdtLocked"),
            Lock::ContentLocked => Some("contentLocked"),
            Lock::Both => Some("sdtContentLocked"),
        }
    }

    pub fn deletable(self) -> bool {
        matches!(self, Lock::None | Lock::ContentLocked)
    }

    pub fn editable(self) -> bool {
        matches!(self, Lock::None | Lock::ControlLocked)
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum ControlKind {
    /// `w:text`.
    PlainText { multi_line: bool },
    /// `w:richText`, or no kind element (Word's default).
    RichText,
    /// `w:picture`.
    Picture,
    /// `w14:checkbox`: the characters it shows checked and unchecked.
    Checkbox { checked: char, unchecked: char },
    /// `w:date`: its display format and language.
    Date { format: String, lid: Option<String> },
    /// `w:dropDownList` or `w:comboBox` (its items in `kind_xml`).
    DropDown,
    /// `w:group`.
    Group,
    /// `w15:repeatingSection`.
    RepeatingSection,
    /// `w15:repeatingSectionItem`: one item (usually a row) of a repeating section.
    RepeatingItem,
    /// A kind the editor doesn't know, or a `w:customXml` wrapper.
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VMerge {
    None,
    Restart,
    Continue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub width: Option<Width>,
    pub span: usize,
    pub v_merge: VMerge,
    pub top: Option<Border>,
    pub left: Option<Border>,
    pub bottom: Option<Border>,
    pub right: Option<Border>,
    pub shading: Option<u32>,
    pub margins: [f64; 4],
    pub v_align: String,
    pub text_direction: String,
    pub no_wrap: bool,
    pub fit_text: bool,
    /// `w:hideMark`.
    pub hide_mark: bool,
    /// The cell's `w:tcPr` as read (or as table formatting changed it): what re-resolution merges over the cell's
    /// table-style region when its place in the table changes.
    pub direct: Raw,
    pub blocks: Vec<Block>,
    /// The cell-level controls around it, outermost first ([`Document::controls`]).
    pub controls: Vec<ControlId>,
    /// Tracked insertion, deletion or merge of the cell, and a tracked change of its properties.
    pub rev: CellRevisions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub height: Option<(f64, String)>,
    pub cant_split: bool,
    pub header: bool,
    pub grid_before: usize,
    pub grid_after: usize,
    pub w_before: Option<f64>,
    pub w_after: Option<f64>,
    pub ind: Option<f64>,
    pub jc: Option<Jc>,
    /// The HTML division the row is in (`trPr/w:divId`).
    pub div: Option<Rc<RowDiv>>,
    /// The table borders the row's exceptions (`w:tblPrEx`) give it, in [`Table::borders`] order; `None` for the
    /// table's.
    pub borders: Option<Rc<[Option<Border>; 6]>>,
    /// The row's exception shading (`w:tblPrEx/w:shd`).
    pub shading: Option<u32>,
    /// Every character of the row, its row-end mark included, is hidden.
    pub vanished: bool,
    /// The row's own cell spacing (`trPr` or `w:tblPrEx`).
    pub cell_spacing: Option<f64>,
    /// The table width of the layout run the row starts or continues, when its exceptions fix its layout.
    pub run: Option<Width>,
    pub cells: Vec<Cell>,
    /// The controls wrapping the row, outermost first.
    pub controls: Vec<ControlId>,
    /// The `w:tr` element's byte range in [`Source::xml`] (editable reads only): saving keeps a row it still has. A
    /// copied row needs `None`, or it would be written as the row it was copied from.
    pub src: Option<Rc<std::ops::Range<usize>>>,
    /// Tracked insertion or deletion of the row, and tracked changes of its properties.
    pub rev: RowRevisions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub style_id: Option<String>,
    pub width: Width,
    pub jc: Jc,
    /// The table's own `w:jc`; a layout run after the first row falls back to it.
    pub own_jc: Option<Jc>,
    pub ind: f64,
    pub ind_set: bool,
    pub borders: [Option<Border>; 6],
    pub fixed: bool,
    pub cell_spacing: f64,
    pub look: Option<Look>,
    pub bands: (i64, i64),
    pub floating: Option<TablePos>,
    pub bidi_visual: bool,
    pub grid: Vec<f64>,
    pub rows: Vec<Row>,
    /// The `w:tbl` element's byte range in [`Source::xml`] (editable reads only).
    pub src: Option<Rc<std::ops::Range<usize>>>,
    /// Identity in layouts ([`next_para_id`]): drawn rows name their table (table handles, border dragging).
    pub id: u64,
    /// The table's `w:tblPr` as read: what re-resolution merges over a new table style's properties.
    pub direct: Raw,
    /// The block-level content controls around it, outermost first.
    pub controls: Vec<ControlId>,
    /// A tracked change of the table's properties, and of its column grid.
    pub change: Option<Rc<TableChange>>,
    pub grid_change: Option<Rc<GridChange>>,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant, reason = "paragraphs dominate; boxing them costs an allocation each")]
pub enum Block {
    Para(Paragraph),
    Table(Table),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionBreak {
    NextPage,
    Continuous,
    EvenPage,
    OddPage,
    NextColumn,
}

/// A section's page: size, margins, columns and a different first page (Page Setup, Layout commands).
#[derive(Clone, Debug, PartialEq)]
pub struct SectionPage {
    pub page_w: f64,
    pub page_h: f64,
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
    pub header: f64,
    pub footer: f64,
    pub gutter: f64,
    pub columns: Columns,
    pub title_pg: bool,
    pub page_borders: Option<PageBorders>,
    /// The `w:pgBorders` as written (what isn't modelled, art borders' ids, is kept), or as last set.
    pub page_borders_xml: Option<String>,
}

impl Section {
    pub fn page(&self) -> SectionPage {
        SectionPage {
            page_w: self.page_w,
            page_h: self.page_h,
            top: self.top,
            bottom: self.bottom,
            left: self.left,
            right: self.right,
            header: self.header,
            footer: self.footer,
            gutter: self.gutter,
            columns: self.columns.clone(),
            title_pg: self.title_pg,
            page_borders: self.page_borders.clone(),
            page_borders_xml: self.page_borders_xml.clone(),
        }
    }

    pub fn set_page(&mut self, p: SectionPage) {
        (self.page_w, self.page_h, self.top, self.bottom, self.left, self.right) = (p.page_w, p.page_h, p.top, p.bottom, p.left, p.right);
        (self.header, self.footer, self.gutter, self.columns, self.title_pg) = (p.header, p.footer, p.gutter, p.columns, p.title_pg);
        (self.page_borders, self.page_borders_xml) = (p.page_borders, p.page_borders_xml);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Columns {
    /// Column widths and the space after each (explicit for unequal columns).
    pub cols: Vec<(f64, f64)>,
    pub equal: bool,
    pub num: usize,
    pub space: f64,
    pub separator: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineNumbering {
    pub count_by: i64,
    pub start: i64,
    pub distance: f64,
    pub restart: String,
}

/// A section's `w:footnotePr` / `w:endnotePr`: number format, start and restart; for footnotes, `w:pos beneathText`.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteProps {
    pub fmt: Option<String>,
    pub start: i64,
    pub restart: NoteRestart,
    pub beneath: bool,
}

impl Default for NoteProps {
    fn default() -> NoteProps {
        NoteProps { fmt: None, start: 1, restart: NoteRestart::Continuous, beneath: false }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoteRestart {
    #[default]
    Continuous,
    EachSect,
    EachPage,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageBorders {
    pub top: Option<Border>,
    pub left: Option<Border>,
    pub bottom: Option<Border>,
    pub right: Option<Border>,
    pub offset_from_page: bool,
    pub display: String,
    pub surround_header: bool,
    pub surround_footer: bool,
}

/// Header/footer references per variant: default, first, even.
pub type HfRefs = [Option<String>; 3];

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub break_kind: SectionBreak,
    pub page_w: f64,
    pub page_h: f64,
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
    pub header: f64,
    pub footer: f64,
    pub gutter: f64,
    pub columns: Columns,
    pub title_pg: bool,
    pub headers: HfRefs,
    pub footers: HfRefs,
    pub pg_start: Option<i64>,
    pub pg_fmt: Option<String>,
    /// `w:pgNumType w:chapStyle` / `w:chapSep`: page numbers lead with the number of the last heading of this level,
    /// then this separator.
    pub chapter: Option<(i64, String)>,
    pub line_numbers: Option<LineNumbering>,
    pub v_align: String,
    pub page_borders: Option<PageBorders>,
    /// The `w:pgBorders` as written, or as last set.
    pub page_borders_xml: Option<String>,
    pub line_grid: Option<i32>,
    /// Footnote and endnote properties, by [`NoteKind`] (foot, end).
    pub notes: [NoteProps; 2],
    /// `w:noEndnote`: the section's endnotes wait for the next section's.
    pub no_endnote: bool,
    pub blocks: Vec<Block>,
    /// A tracked change of the section's properties.
    pub change: Option<Rc<SectionChange>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Compat {
    #[default]
    Legacy,
    Generic,
}

#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub compat_mode: i64,
    /// `w:compat` child options that are on.
    pub compat: std::collections::HashSet<String>,
    pub default_tab: f64,
    /// `w:characterSpacingControl` is compressPunctuation or compressPunctuationAndJapaneseKana.
    pub compress_punctuation: bool,
    pub even_and_odd: bool,
    pub mirror_margins: bool,
    pub gutter_at_top: bool,
    /// `w:bookFoldPrinting` / `w:bookFoldRevPrinting`: `Some(reversed)`.
    pub book_fold: Option<bool>,
    pub auto_hyphenation: bool,
    pub hyphenation_zone: f64,
    pub consecutive_hyphen_limit: i64,
    pub do_not_hyphenate_caps: bool,
    pub borders_do_not_surround_header: bool,
    pub borders_do_not_surround_footer: bool,
    /// `w:trackRevisions`: edits are recorded as revisions.
    pub track_revisions: bool,
    /// `w:endnotePr/w:pos sectEnd`: endnotes follow their section rather than the document.
    pub endnotes_sect_end: bool,
    pub protection: Option<Protection>,
    /// `w:revisionView` as written, kept for saving.
    pub revision_view_raw: Raw,
}

impl Settings {
    pub fn compat(&self) -> Compat {
        if self.compat_mode >= 15 { Compat::Generic } else { Compat::Legacy }
    }

    pub fn has(&self, flag: &str) -> bool {
        self.compat.contains(flag)
    }
}

/// A list definition made since reading: its `w:abstractNum` (as written) and the `w:num` that uses it.
#[derive(Clone, Debug, PartialEq)]
pub struct AddedList {
    pub abstract_id: i64,
    pub num_id: i64,
    pub abstract_xml: String,
}

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub sections: Vec<Section>,
    pub theme: Rc<crate::drawing::theme::Theme>,
    pub colour_map: crate::drawing::theme::ColourMap,
    pub page_colour: Option<u32>,
    /// Headers and footers by part name, in name order (the editor addresses them by place in it).
    pub stories: std::collections::BTreeMap<String, Vec<Block>>,
    pub footnotes: HashMap<i64, Vec<Block>>,
    pub endnotes: HashMap<i64, Vec<Block>>,
    /// Footnote separator / continuation separator stories, when the document defines them.
    pub separators: HashMap<String, Vec<Block>>,
    pub images: Vec<Vec<u8>>,
    /// The hyphenation patterns the host supplied.
    pub hyphenation: Option<crate::layout::hyphenate::Hyphenation>,
    /// Content controls the model keeps: row-level wrappers (repeating sections and their items) and checkboxes,
    /// by id.
    pub controls: HashMap<ControlId, Control>,
    /// The package's custom XML parts: definition data the host keeps with the template.
    pub data_parts: Vec<DataPart>,
    /// The glossary part's building blocks, when the package has one.
    pub glossary: Option<Glossary>,
    /// The item parts of data parts removed since reading (taken out of the package on save).
    pub removed_data: Vec<String>,
    /// Placeholders filled since reading, by image index: drawn as their image, saved without
    /// `w:showingPlcHdr`.
    pub filled: std::collections::HashSet<usize>,
    /// Sections whose page ([`SectionPage`]) was edited: saving writes their `w:sectPr` page properties.
    pub edited_sections: std::collections::HashSet<usize>,
    /// How many sections as read each section holds: more than one once section breaks were deleted. Empty: one
    /// each (nothing deleted).
    pub section_sources: Vec<usize>,
    /// Pictures that can't be deleted, by image index: those in picture controls that can't be deleted (`w:lock`
    /// `sdtLocked` or `sdtContentLocked`).
    pub undeletable: std::collections::HashSet<usize>,
    /// Pictures in picture controls, by image index: the innermost picture control of each picture showing it.
    pub controlled: HashMap<usize, Vec<ControlId>>,
    pub settings: Settings,
    pub line_number_run: Option<Run>,
    /// What the document was read from, when read for editing.
    pub source: Option<Rc<Source>>,
    /// The styles, numbering and theme it resolves with. `Rc`: clipboard fragments carry their source
    /// document's resolver to resolve pasted formatting (plan, Complexity Tracking).
    pub resolver: Option<Rc<super::resolve::Resolver>>,
    /// The font table's description of each family, by lowercase name: what a resolver is told about a missing one.
    pub font_descriptions: HashMap<String, super::font_table::FontDescription>,
    /// Families the style, numbering, theme and font table parts were rewritten from and to, in order: the save
    /// rewrites those parts.
    pub font_renames: Vec<super::font_table::FontRename>,
    /// Lists made by the Numbering and Bullets commands since reading: written into the numbering part.
    pub added_lists: Vec<AddedList>,
    /// Who made the document's revisions and comments.
    pub people: People,
    pub comments: Comments,
    /// How the review shows (markup view, author colours): a view setting, never saved.
    pub review: ReviewView,
    /// New revision, comment and paragraph ids, past those read.
    pub review_ids: IdCounter,
    /// What reading couldn't keep or make sense of.
    pub diagnostics: super::diagnostics::Diagnostics,
}

impl Document {
    /// The bodies of the glossary's building blocks that haven't been removed, in glossary order.
    pub fn glossary_bodies(&self) -> impl Iterator<Item = &Vec<Block>> {
        self.glossary.iter().flat_map(|g| g.parts.iter()).filter(|b| b.state != PartState::Removed).map(|b| &b.body)
    }

    /// Rebuilds the image sets kept for picture controls (`controlled`, `undeletable`) from the controls holding the
    /// pictures, so they follow the controls' locks. Run after reading, and after a transaction that changes a control
    /// or moves pictures.
    pub fn derive_image_sets(&mut self) {
        self.undeletable.clear();
        self.controlled.clear();
        for (img, c) in self.picture_controls() {
            self.controlled.entry(img).or_default().push(c);
            if matches!(self.controls[&c].props.lock, Lock::ControlLocked | Lock::Both) {
                self.undeletable.insert(img);
            }
        }
    }

    /// Each picture in a picture control, by image index, with its innermost picture control.
    pub fn picture_controls(&self) -> Vec<(usize, ControlId)> {
        if !self.controls.values().any(|c| c.props.kind == ControlKind::Picture) {
            return Vec::new();
        }
        type Found = Vec<(usize, ControlId)>;
        // The innermost picture control around an inline: an open inline one, else the paragraph's.
        fn pick(open: &[ControlId], block: Option<ControlId>, controls: &HashMap<ControlId, Control>) -> Option<ControlId> {
            open.iter().rev().copied().find(|c| controls.get(c).is_some_and(|k| k.props.kind == ControlKind::Picture)).or(block)
        }
        fn walk(blocks: &[Block], controls: &HashMap<ControlId, Control>, out: &mut Found) {
            for b in blocks {
                match b {
                    Block::Para(p) => {
                        let block = p.controls.iter().rev().copied().find(|c| controls.get(c).is_some_and(|k| k.props.kind == ControlKind::Picture));
                        let mut open: Vec<ControlId> = Vec::new();
                        for i in &p.inlines {
                            match i {
                                Inline::ControlStart(c) => open.push(*c),
                                Inline::ControlEnd(c) => {
                                    if let Some(k) = open.iter().rposition(|x| x == c) {
                                        open.truncate(k);
                                    }
                                }
                                Inline::Picture(pic, _) => {
                                    if let (Some(img), Some(c)) = (pic.image, pick(&open, block, controls)) {
                                        out.push((img, c));
                                    }
                                }
                                Inline::Float(f, _) => {
                                    if let (Some(img), Some(c)) = (f.image, pick(&open, block, controls)) {
                                        out.push((img, c));
                                    }
                                    if let Some(tb) = &f.text_box {
                                        walk(&tb.blocks, controls, out);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, controls, out)),
                }
            }
        }
        let mut found = Found::new();
        let stories =
            self.sections.iter().map(|s| &s.blocks).chain(self.stories.values()).chain(self.footnotes.values()).chain(self.endnotes.values());
        stories.for_each(|b| walk(b, &self.controls, &mut found));
        found
    }
}

/// A `w:ptab`'s position: left, centre or right (`align`) of the text area (`margin`) or of the paragraph's indents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PTab {
    pub align: TabKind,
    pub margin: bool,
    pub leader: Leader,
}

impl Inline {
    /// The run that formats the inline (none for bookmarks).
    pub fn run(&self) -> Option<&Run> {
        match self {
            Inline::Text(_, r)
            | Inline::Tab(r)
            | Inline::PTab(_, r)
            | Inline::Break(_, r)
            | Inline::Picture(_, r)
            | Inline::Float(_, r)
            | Inline::NoteMark(_, _, r)
            | Inline::Ruby(_, r) => Some(r),
            Inline::Sym { run, .. }
            | Inline::NoteRef { run, .. }
            | Inline::Separator { run, .. }
            | Inline::FieldBegin { run, .. }
            | Inline::FieldSep { run, .. }
            | Inline::FieldEnd { run, .. }
            | Inline::CommentRef { run, .. } => Some(run),
            Inline::Bookmark(_) | Inline::Range(_) | Inline::ControlStart(_) | Inline::ControlEnd(_) => None,
        }
    }

    /// Caret units the inline occupies: a character per character of text, one per other drawn inline, none for
    /// bookmarks, range markers, comment references and anchored objects.
    pub fn units(&self) -> usize {
        match self {
            Inline::Text(s, _) => s.chars().count(),
            Inline::Bookmark(_)
            | Inline::Range(_)
            | Inline::CommentRef { .. }
            | Inline::Float(..)
            | Inline::ControlStart(_)
            | Inline::ControlEnd(_) => 0,
            _ => 1,
        }
    }
}

impl Tab {
    pub fn is_clear(&self) -> bool {
        self.kind == TabKind::Clear
    }
}

impl Leader {
    pub fn glyph(self) -> Option<char> {
        match self {
            Leader::None => None,
            Leader::Dot => Some('.'),
            Leader::Hyphen => Some('-'),
            Leader::Underscore => Some('_'),
            Leader::MiddleDot => Some('·'),
        }
    }
}
