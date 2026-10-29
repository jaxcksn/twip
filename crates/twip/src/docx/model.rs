//! The layout engine's input: a document with every style resolved (the STYLE rules). Lengths are points.

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
    /// The East Asian font is the theme's empty slot (`east_asia` is then the Latin font): Word treats it as an East
    /// Asian font for balanced space runs (FONT-14).
    pub east_asia_unset: bool,
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

#[derive(Clone, Debug, PartialEq)]
pub struct ParaStyle {
    pub style_id: Option<String>,
    pub jc: Jc,
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
    pub keep_next: bool,
    pub keep_lines: bool,
    pub page_break_before: bool,
    pub widow_control: bool,
    pub suppress_auto_hyphens: bool,
    pub suppress_line_numbers: bool,
    pub tabs: Vec<Tab>,
    pub border_top: Option<Border>,
    pub border_bottom: Option<Border>,
    pub border_left: Option<Border>,
    pub border_right: Option<Border>,
    pub border_between: Option<Border>,
    pub shading: Option<u32>,
    pub frame: Option<FramePr>,
    pub text_alignment: String,
    pub outline_lvl: Option<i64>,
    /// The list level's `w:lvlJc` (LIST-1): the number starts (left), ends (right) or is centred at the first-line position.
    pub list_jc: String,
    /// The list the paragraph is in (numId, ilvl; direct or from its style), and that level's `w:numFmt`.
    pub num: Option<(i64, i64)>,
    pub list_fmt: Option<String>,
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

/// Fields the engine evaluates (FIELD-1): layout draws their value in place of their result; every other field shows its
/// result.
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
    /// A code whose first word can be neither a field name nor a bookmark (`"http://…"` left by a merge): Word shows
    /// nothing.
    Invalid,
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
}

/// An anchored (floating) picture or text box (the FLOAT rules).
#[derive(Clone, Debug, PartialEq)]
pub struct Float {
    /// Identity in layouts and editing ([`next_para_id`]'s counter): fragments name it, selections and drags keep it.
    pub id: u64,
    pub w: f64,
    /// A frame without `w`: as wide as its content, at most the column (FRAME-1).
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
    pub behind: bool,
    pub allow_overlap: bool,
    pub layout_in_cell: bool,
    /// Document order of the float, for F13.
    pub order: usize,
    /// The source `w:drawing` element's byte range in its part: saving drops it when the float is deleted.
    pub src: Option<std::ops::Range<usize>>,
    /// `wp:anchor/@locked`: the anchor stays in its paragraph when the float is moved.
    pub locked: bool,
    /// A frame's paragraphs in the source (frames.rs): saving drops them when the frame is deleted.
    pub frame_src: Option<std::ops::Range<usize>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBox {
    pub blocks: Vec<Block>,
    pub insets: [f64; 4],
    pub outline: f64,
    pub auto_fit: bool,
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
    /// `w:separator` / `w:continuationSeparator` in a note separator story: the rule above the notes (NOTE-3).
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
        /// An evaluated kind's value computed after reading (SEQ, REF, IF, DATE, STYLEREF, FIELD-1), shown in place of
        /// the result; page-dependent kinds get theirs at layout.
        value: Option<String>,
        /// The `begin` run's formatting; for an evaluated kind, what its value is drawn in (FIELD-4: the result's first
        /// run with `\* MERGEFORMAT`, else the code's).
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListLabel {
    pub text: String,
    pub run: Run,
    pub jc: String,
    pub suff: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    pub style: Rc<ParaStyle>,
    /// The paragraph mark's formatting (empty-line height, STYLE-5).
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
    /// Its `w:p` elements: several when hidden paragraph marks joined them (STYLE-7).
    pub ranges: Vec<std::ops::Range<usize>>,
    /// The elements that produced its units, in order, with how many units each did: `w:t`, `w:tab`, a drawing, a
    /// whole complex field (from its `begin` run to its `end` run), ...
    pub segs: Vec<(std::ops::Range<usize>, usize)>,
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
    /// `w:wBefore` / `w:wAfter`: the skipped columns' width, points (0: none). Autofit only (TABLE-15).
    pub w_before: f64,
    pub w_after: f64,
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
    pub ind: f64,
    pub borders: [Option<Border>; 6],
    pub fixed: bool,
    pub cell_spacing: f64,
    pub look: Option<Look>,
    /// `w:tblStyleRowBandSize` and `w:tblStyleColBandSize` (table style banding, STYLE-4).
    pub bands: (i64, i64),
    /// `w:tblpPr`: placed at its anchor when it starts a column; text doesn't wrap beside it (TABLE-49).
    pub floating: Option<TablePos>,
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
        }
    }

    pub fn set_page(&mut self, p: SectionPage) {
        (self.page_w, self.page_h, self.top, self.bottom, self.left, self.right) = (p.page_w, p.page_h, p.top, p.bottom, p.left, p.right);
        (self.header, self.footer, self.gutter, self.columns, self.title_pg) = (p.header, p.footer, p.gutter, p.columns, p.title_pg);
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
    pub line_numbers: Option<LineNumbering>,
    pub v_align: String,
    pub page_borders: Option<PageBorders>,
    pub blocks: Vec<Block>,
    /// A tracked change of the section's properties.
    pub change: Option<Rc<SectionChange>>,
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
    pub auto_hyphenation: bool,
    pub hyphenation_zone: f64,
    pub consecutive_hyphen_limit: i64,
    pub do_not_hyphenate_caps: bool,
    pub borders_do_not_surround_header: bool,
    pub borders_do_not_surround_footer: bool,
    /// `w:trackRevisions`: edits are recorded as revisions.
    pub track_revisions: bool,
    pub protection: Option<Protection>,
    /// `w:revisionView` as written, kept for saving.
    pub revision_view_raw: Raw,
}

impl Settings {
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
    /// Headers and footers by part name, in name order (the editor addresses them by place in it).
    pub stories: std::collections::BTreeMap<String, Vec<Block>>,
    pub footnotes: HashMap<i64, Vec<Block>>,
    pub endnotes: HashMap<i64, Vec<Block>>,
    /// Footnote separator / continuation separator stories, when the document defines them.
    pub separators: HashMap<String, Vec<Block>>,
    pub images: Vec<Vec<u8>>,
    /// Content controls the model keeps: row-level wrappers (repeating sections and their items) and checkboxes,
    /// by id.
    pub controls: HashMap<ControlId, Control>,
    /// The package's custom XML parts: definition data the host keeps with the template.
    pub data_parts: Vec<DataPart>,
    /// The item parts of data parts removed since reading (taken out of the package on save).
    pub removed_data: Vec<String>,
    /// Pictures of merge images and placeholders still showing their box (`w:showingPlcHdr`): image index →
    /// (merge image, label). The editor draws them as labeled boxes.
    pub placeholders: HashMap<usize, (bool, String)>,
    /// The placeholders' keys (their tag after `Placeholder:` or `MergeImage:`), by image index.
    pub placeholder_keys: HashMap<usize, String>,
    /// Placeholders filled since reading, by image index: drawn as their image, saved without
    /// `w:showingPlcHdr`.
    pub filled: std::collections::HashSet<usize>,
    /// Pictures that can't be resized or moved, by image index: those in placeholder and merge image controls (filled
    /// or not; the ones a click acts on) and in picture controls whose content is locked (`w:lock`).
    pub fixed: std::collections::HashSet<usize>,
    /// Sections whose page ([`SectionPage`]) was edited: saving writes their `w:sectPr` page properties.
    pub edited_sections: std::collections::HashSet<usize>,
    /// Pictures that can't be deleted, by image index: those in picture controls that can't be deleted (`w:lock`
    /// `sdtLocked` or `sdtContentLocked`).
    pub undeletable: std::collections::HashSet<usize>,
    pub settings: Settings,
    /// The `LineNumber` character style, resolved (LNUM-1).
    pub line_number_run: Option<Run>,
    /// What the document was read from, when read for editing.
    pub source: Option<Rc<Source>>,
    /// The styles, numbering and theme it resolves with. `Rc`: clipboard fragments carry their source
    /// document's resolver to resolve pasted formatting (plan, Complexity Tracking).
    pub resolver: Option<Rc<super::resolve::Resolver>>,
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
    /// Rebuilds the image sets kept for picture controls (`fixed`, `undeletable`, `placeholders`,
    /// `placeholder_keys`) from the controls holding the pictures, so they follow the controls' tags and locks (spec
    /// 003 C5). Run after reading, and after a transaction that changes a control or moves pictures.
    pub fn derive_image_sets(&mut self) {
        self.fixed.clear();
        self.undeletable.clear();
        self.placeholders.clear();
        self.placeholder_keys.clear();
        if !self.controls.values().any(|c| c.props.kind == ControlKind::Picture) {
            return;
        }
        type Found = Vec<(usize, bool, ControlId)>;
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
                                        out.push((img, true, c));
                                    }
                                }
                                Inline::Float(f, _) => {
                                    if let (Some(img), Some(c)) = (f.image, pick(&open, block, controls)) {
                                        out.push((img, false, c));
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
        for (img, inline, c) in found {
            let props = &self.controls[&c].props;
            // `Placeholder:key` and `MergeImage:key` tags.
            let (kind, key) = props
                .tag
                .as_deref()
                .and_then(|t| t.split_once(':'))
                .map_or(("", ""), |(k, rest)| (k.trim(), rest.split(';').next().unwrap_or("").trim()));
            let merge = kind.eq_ignore_ascii_case("MergeImage");
            let placeholder = merge || kind.eq_ignore_ascii_case("Placeholder");
            if placeholder || matches!(props.lock, Lock::ContentLocked | Lock::Both) {
                self.fixed.insert(img);
            }
            if matches!(props.lock, Lock::ControlLocked | Lock::Both) {
                self.undeletable.insert(img);
            }
            if inline && placeholder && props.showing_placeholder {
                self.placeholders.insert(img, (merge, props.alias.clone().unwrap_or_else(|| key.to_string())));
                self.placeholder_keys.insert(img, key.to_string());
            }
        }
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
            | Inline::NoteMark(_, _, r) => Some(r),
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
