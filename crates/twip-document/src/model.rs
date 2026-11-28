//! The document tree: the layout engine's input, a document with every style resolved (the STYLE rules). Lengths
//! are points.

pub use crate::anchor::{FloatWrap, HorizontalAlign, HorizontalRelation, VerticalAlign, VerticalRelation, WrapText};
pub use crate::content::{BreakKind, NoteKind};
pub use crate::foreign::{Foreign, FormatId};
pub use crate::format::{TextEffects, ThemeColourRef};
pub use crate::ident::{ControlId, FieldId, HeaderFooterId, ResourceId, next_para_id};
pub use crate::line::Underline;
pub use crate::origin::Origin;
pub use crate::paragraph::{ListId, ListJc, ListRef, ListSuffix, TextAlignment};
pub use crate::policy::{Compat, LayoutPolicy};
pub use crate::resource::Resources;
pub use crate::section::{HeightRule, LineNumberingRestart, PageBorderDisplay, VerticalAlignment};

use crate::fonts::GenericFamily;
use std::collections::HashMap;
use std::rc::Rc;

use crate::format::{Border, FramePr, Jc, Leader, LineRule, Look, Tab, TabKind, TablePos, VertAlign, Width};
use crate::review::{
    CellRevisions, CommentId, Comments, GridChange, IdCounter, ParaChange, People, Protection, RangeMark, RowRevisions, RunRevisions, SectionChange,
    TableChange,
};

/// Effective character formatting of a run. Its `==` is [`RunStyle::layout_eq`].
#[derive(Clone, Debug)]
pub struct RunStyle {
    /// Family names per script slot, before font substitution.
    pub ascii: String,
    /// The high ANSI slot's family.
    pub h_ansi: String,
    /// The East Asian slot's family.
    pub east_asia: String,
    /// The complex script slot's family.
    pub cs: String,
    /// The East Asian slot names a theme font the theme leaves unset, and nothing is set below it.
    pub east_asia_unset: bool,
    /// The East Asian slot names a family of its own (not unset, not Times New Roman).
    pub east_asia_named: bool,
    /// The run's font hint is East Asian (`w:rFonts w:hint="eastAsia"`).
    pub hint_east_asia: bool,
    /// Size, points.
    pub size: f64,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// All capitals.
    pub caps: bool,
    /// Small capitals.
    pub small_caps: bool,
    /// Struck through.
    pub strike: bool,
    /// Struck through twice.
    pub dstrike: bool,
    /// Hidden (`w:vanish`).
    pub hidden: bool,
    /// `w:noProof`: no automatic hyphenation either.
    pub no_proof: bool,
    /// The underline, when there is one.
    pub underline: Option<Underline>,
    /// The underline's own colour (`w:u/@w:color`); `None`: the text's.
    pub underline_color: Option<u32>,
    /// The theme colour `underline_color` is.
    pub underline_color_theme: Option<ThemeColourRef>,
    /// Emboss, engrave (`w:imprint`), outline and shadow.
    pub emboss: bool,
    /// Engrave.
    pub imprint: bool,
    /// Outline.
    pub outline: bool,
    /// Shadow.
    pub shadow: bool,
    /// The OpenType features as set, whatever the compatibility mode makes of them ([`RunStyle::ligatures`] and
    /// [`RunStyle::figures`] are what layout applies).
    pub typography: Typography,
    /// Word 2010's text effects through the style chain; `None` without any.
    pub effects: Option<Rc<TextEffects>>,
    /// The text colour (`0xRRGGBB`); `None`: automatic.
    pub color: Option<u32>,
    /// The theme colour `color` is.
    pub color_theme: Option<ThemeColourRef>,
    /// The theme font the Latin text's family is (`w:asciiTheme`), when it is one.
    pub font_theme: Option<ThemeFont>,
    /// The highlight colour (`0xRRGGBB`).
    pub highlight: Option<u32>,
    /// The shading's resolved colour (`0xRRGGBB`).
    pub shading: Option<u32>,
    /// The shading's pattern, pattern color and fill, as written.
    pub shd: Option<crate::format::Shd>,
    /// The text's border (`w:bdr`): a box around runs in a row with the same one.
    pub border: Option<crate::format::Border>,
    /// Superscript, subscript or the baseline.
    pub vert_align: VertAlign,
    /// Raise (+) / lower (−), points.
    pub position: f64,
    /// Extra character spacing, points.
    pub spacing: f64,
    /// Horizontal scale, percent.
    pub scale: f64,
    /// Kern at or above this size; 0 = off.
    pub kern: f64,
    /// The OpenType ligatures layout applies: the bits of [`Typography::ligatures`].
    pub ligatures: u8,
    /// OpenType number and style features asked for: 1 `lnum`, 2 `onum`, 4 `pnum`, 8 `tnum`, and `16 << (n − 1)` for
    /// `ssNN`.
    pub figures: u32,
    /// `w:fitText`: the width its region of text is fitted to (points) and the region's id.
    pub fit_text: Option<(f64, i64)>,
    /// `w:em`, when it marks the text.
    pub em: Option<crate::format::Emphasis>,
    /// In a table of contents' result: the Hyperlink character style doesn't show (the DOCX adapter's `run_xml::rerun`).
    pub toc: bool,
    /// The language.
    pub lang: String,
    /// The East Asian language (`w:lang w:eastAsia`).
    pub lang_east_asia: String,
    /// The character style.
    pub style_id: Option<String>,
    /// The authored properties, with the `w:rPr` they were read as (editable reads only): what saving writes back
    /// for new runs in this formatting.
    pub direct: crate::format::RunDirect,
    /// Tracked insertion, deletion and formatting change of the run (or, on a paragraph's mark, of the mark).
    pub rev: RunRevisions,
    /// The generic categories the document's font table gives the families of the `ascii`, `h_ansi` and `east_asia`
    /// slots.
    pub categories: [Option<GenericFamily>; 3],
}

impl RunStyle {
    /// Layout equivalence: every resolved value, the style and the revision marks, which the markup views draw. The
    /// authored properties ([`RunStyle::direct`]) are not compared.
    pub fn layout_eq(&self, o: &RunStyle) -> bool {
        let RunStyle {
            ascii,
            h_ansi,
            east_asia,
            cs,
            east_asia_unset,
            east_asia_named,
            hint_east_asia,
            size,
            bold,
            italic,
            caps,
            small_caps,
            strike,
            dstrike,
            hidden,
            no_proof,
            underline,
            underline_color,
            underline_color_theme,
            emboss,
            imprint,
            outline,
            shadow,
            typography,
            effects,
            color,
            color_theme,
            font_theme,
            highlight,
            shading,
            shd,
            border,
            vert_align,
            position,
            spacing,
            scale,
            kern,
            ligatures,
            figures,
            fit_text,
            em,
            toc,
            lang,
            lang_east_asia,
            style_id,
            direct: _,
            rev,
            categories,
        } = self;
        *ascii == o.ascii
            && *h_ansi == o.h_ansi
            && *east_asia == o.east_asia
            && *cs == o.cs
            && *east_asia_unset == o.east_asia_unset
            && *east_asia_named == o.east_asia_named
            && *hint_east_asia == o.hint_east_asia
            && *size == o.size
            && *bold == o.bold
            && *italic == o.italic
            && *caps == o.caps
            && *small_caps == o.small_caps
            && *strike == o.strike
            && *dstrike == o.dstrike
            && *hidden == o.hidden
            && *no_proof == o.no_proof
            && *underline == o.underline
            && *underline_color == o.underline_color
            && *underline_color_theme == o.underline_color_theme
            && *emboss == o.emboss
            && *imprint == o.imprint
            && *outline == o.outline
            && *shadow == o.shadow
            && *typography == o.typography
            && *effects == o.effects
            && *color == o.color
            && *color_theme == o.color_theme
            && *font_theme == o.font_theme
            && *highlight == o.highlight
            && *shading == o.shading
            && *shd == o.shd
            && *border == o.border
            && *vert_align == o.vert_align
            && *position == o.position
            && *spacing == o.spacing
            && *scale == o.scale
            && *kern == o.kern
            && *ligatures == o.ligatures
            && *figures == o.figures
            && *fit_text == o.fit_text
            && *em == o.em
            && *toc == o.toc
            && *lang == o.lang
            && *lang_east_asia == o.lang_east_asia
            && *style_id == o.style_id
            && *rev == o.rev
            && *categories == o.categories
    }

    /// Authored equality: the direct formatting as written, and the revision marks with the formatting a format
    /// change records as written.
    pub fn authored_eq(&self, o: &RunStyle) -> bool {
        let (a, b) = (&self.rev, &o.rev);
        self.direct.authored_eq(&o.direct)
            && a.ins == b.ins
            && a.del == b.del
            && match (&a.fmt, &b.fmt) {
                (None, None) => true,
                (Some(x), Some(y)) => x.mark == y.mark && x.previous.direct.authored_eq(&y.previous.direct),
                _ => false,
            }
    }

    /// Both relations: what text joining one run needs.
    pub fn same_run(&self, o: &RunStyle) -> bool {
        self.layout_eq(o) && self.authored_eq(o)
    }
}

impl PartialEq for RunStyle {
    fn eq(&self, o: &RunStyle) -> bool {
        self.layout_eq(o)
    }
}

impl RunStyle {
    /// The generic category the document's font table gives `family`, when it is one of this run's slots.
    pub fn category(&self, family: &str) -> Option<GenericFamily> {
        [&self.ascii, &self.h_ansi, &self.east_asia].iter().position(|f| f.as_str() == family).and_then(|k| self.categories[k])
    }
}

/// A run's effective formatting, shared between the inlines it formats.
pub type Run = Rc<RunStyle>;

/// One of a theme's two font sets: its headings' or its body text's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeFont {
    /// The heading font (`major`).
    Major,
    /// The body font (`minor`).
    Minor,
}

impl ThemeFont {
    /// The theme font a `w:asciiTheme`-style value names.
    pub fn of(v: &str) -> Option<ThemeFont> {
        if v.starts_with("major") {
            Some(ThemeFont::Major)
        } else if v.starts_with("minor") {
            Some(ThemeFont::Minor)
        } else {
            None
        }
    }
}

/// A run's OpenType settings as written (`w14:ligatures`, `w14:cntxtAlts`, `w14:numForm`, `w14:numSpacing`,
/// `w14:stylisticSets`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Typography {
    /// 1 standard, 2 contextual, 4 historical, 8 discretionary.
    pub ligatures: u8,
    /// Contextual alternates.
    pub contextual_alternates: bool,
    /// 1 lining, 2 old style, 0 the font's default.
    pub number_form: u32,
    /// 4 proportional, 8 tabular, 0 the font's default.
    pub number_spacing: u32,
    /// Bit n − 1 for stylistic set n.
    pub stylistic_sets: u32,
}

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
    /// The division's id.
    pub id: i64,
    /// How far the division's left margins move the row right, points.
    pub shift: f64,
    /// Space above the row: the divisions' top margins and box borders opening there, points.
    pub lead: f64,
    /// Space below the row: the divisions' bottom margins and box borders closing there, points.
    pub trail: f64,
    /// The bordered divisions around the row, outermost first.
    pub boxes: Rc<[DivBox]>,
}

/// An HTML division (`w:div` in `webSettings`).
#[derive(Clone, Debug, PartialEq)]
pub struct Division {
    /// The division it is in.
    pub parent: Option<i64>,
    /// Left, right, top and bottom margins.
    pub margins: [f64; 4],
    /// Top, left, bottom and right borders.
    pub borders: [Option<Border>; 4],
}

/// What the divisions starting and ending at a paragraph add to it, which depends on its neighbours: space above and
/// below at least `before` and `after`, and the boxes around it. `style` is `from` with them applied.
#[derive(Clone, Debug, PartialEq)]
pub struct DivEdge {
    /// The least space above the paragraph, points.
    pub before: f64,
    /// The least space below the paragraph, points.
    pub after: f64,
    /// The bordered divisions around the paragraph, outermost first.
    pub boxes: Rc<[DivBox]>,
    from: Rc<ParaStyle>,
    style: Rc<ParaStyle>,
}

impl DivEdge {
    /// The edge over style `from`: space before and after at least `before` and `after`, inside `boxes`.
    pub fn new(from: &Rc<ParaStyle>, before: f64, after: f64, boxes: Rc<[DivBox]>) -> DivEdge {
        let mut s = (**from).clone();
        s.space_before = s.space_before.max(before);
        s.space_after = s.space_after.max(after);
        s.div_boxes = boxes.clone();
        DivEdge { before, after, boxes, from: from.clone(), style: Rc::new(s) }
    }

    /// The paragraph's style with the edge applied, while `style` is the one it was built over.
    pub fn over<'a>(&'a self, style: &'a Rc<ParaStyle>) -> &'a Rc<ParaStyle> {
        if Rc::ptr_eq(&self.from, style) { &self.style } else { style }
    }
}

/// A bordered HTML division's box, as one paragraph in it sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct DivBox {
    /// Top, left, bottom, right.
    pub borders: [Option<Border>; 4],
    /// The division's left and right edges, from the text column's.
    pub left: f64,
    /// The right edge.
    pub right: f64,
    /// The box opens above this paragraph, closes below it.
    pub starts: bool,
    /// The box closes below the paragraph.
    pub ends: bool,
}

/// Effective paragraph formatting. Its `==` is [`ParaStyle::layout_eq`].
#[derive(Clone, Debug)]
pub struct ParaStyle {
    /// The paragraph style.
    pub style_id: Option<String>,
    /// Alignment.
    pub jc: Jc,
    /// Right to left.
    pub bidi: bool,
    /// Left indent, points.
    pub ind_left: f64,
    /// Right indent, points.
    pub ind_right: f64,
    /// + first line, − hanging.
    pub ind_first: f64,
    /// Space before, points.
    pub space_before: f64,
    /// Space after, points.
    pub space_after: f64,
    /// Space before is automatic (`w:beforeAutospacing`).
    pub before_auto: bool,
    /// Space after is automatic (`w:afterAutospacing`).
    pub after_auto: bool,
    /// Auto: multiple (1.0 = single); exact / at least: points.
    pub line: f64,
    /// How [`ParaStyle::line`] is measured.
    pub line_rule: LineRule,
    /// No space before or after between paragraphs of the same style (`w:contextualSpacing`).
    pub contextual: bool,
    /// Lines snap to the document grid.
    pub snap_to_grid: bool,
    /// Kept on the page with the next paragraph.
    pub keep_next: bool,
    /// Its lines are kept on one page.
    pub keep_lines: bool,
    /// Starts a page.
    pub page_break_before: bool,
    /// Widow and orphan control.
    pub widow_control: bool,
    /// No automatic hyphenation.
    pub suppress_auto_hyphens: bool,
    /// `w:kinsoku` off: no East Asian line-breaking classes.
    pub no_kinsoku: bool,
    /// `w:overflowPunct` off: no punctuation hangs past the right margin.
    pub no_overflow_punct: bool,
    /// `w:autoSpaceDE` / `w:autoSpaceDN` off: no extra space between East Asian text and Latin letters / digits.
    pub no_auto_space_de: bool,
    /// No extra space between East Asian text and digits.
    pub no_auto_space_dn: bool,
    /// `w:topLinePunct`: an opening bracket starting a line in an East Asian paragraph loses its left half.
    pub top_line_punct: bool,
    /// `w:wordWrap` off: East Asian and Latin text may break between any two characters.
    pub no_word_wrap: bool,
    /// `w:textboxTightWrap` in a text box: 0 none, 1 all lines, 2 first and last, 3 first only, 4 last only.
    pub tight_wrap: u8,
    /// `w:suppressOverlap`: the paragraph's frame may not overlap other frames and floats.
    pub suppress_overlap: bool,
    /// Its lines aren't numbered.
    pub suppress_line_numbers: bool,
    /// `w:mirrorIndents`: on mirrored pages the indents swap sides.
    pub mirror_indents: bool,
    /// The HTML division the paragraph is in (`w:divId`).
    pub div: Option<i64>,
    /// The bordered divisions around the paragraph, outermost first (only in [`Paragraph::para_style_in`]'s style:
    /// they come from the neighbours, [`DivEdge`]).
    pub div_boxes: Rc<[DivBox]>,
    /// Tab stops.
    pub tabs: Vec<Tab>,
    /// Top border.
    pub border_top: Option<Border>,
    /// Bottom border.
    pub border_bottom: Option<Border>,
    /// Left border.
    pub border_left: Option<Border>,
    /// Right border.
    pub border_right: Option<Border>,
    /// The border between paragraphs with the same borders.
    pub border_between: Option<Border>,
    /// The shading's resolved colour (`0xRRGGBB`).
    pub shading: Option<u32>,
    /// The shading's pattern, pattern color and fill, as written.
    pub shd: Option<crate::format::Shd>,
    /// The frame the paragraph is in (`w:framePr`).
    pub frame: Option<FramePr>,
    /// The `w:textDirection` text-flow code (`text_flow`): a frame's paragraphs run in it.
    pub text_flow: u8,
    /// The paragraph is in a frame: its `frame` made the float that holds it.
    pub in_frame: bool,
    /// Vertical alignment of characters on a line.
    pub text_alignment: TextAlignment,
    /// The outline level.
    pub outline_lvl: Option<i64>,
    /// `w15:collapsed`: the heading opens collapsed.
    pub collapsed: bool,
    /// The list label's alignment.
    pub list_jc: ListJc,
    /// The list the paragraph is in (direct or from its style), and that level's `w:numFmt`.
    pub num: Option<ListRef>,
    /// The list level's number format, as written.
    pub list_fmt: Option<String>,
    /// The list's id, as written.
    pub list: Option<i64>,
    /// The paragraph mark's size, points, when it differs from the text's.
    pub mark_size: Option<f64>,
    /// The authored properties (editable reads only), what formatting edits change and saving writes back.
    pub direct: crate::format::ParaDirect,
    /// A tracked change of the paragraph's properties.
    pub change: Option<Rc<ParaChange>>,
    /// Paragraphs joined onto this one's start through vanished marks, when each line takes its indents, alignment and
    /// tabs from the paragraph its first character is in: the unit each one after the first starts at, and its own
    /// style. Empty otherwise.
    pub joined: Rc<[(u32, Rc<ParaStyle>)]>,
    /// Paragraphs joined onto this one's start through vanished marks: each one's own style, this one's last, as they
    /// show while hidden text does. Empty otherwise; a chain longer than [`crate::format::resolve::MAX_PARTS`] keeps its first
    /// ones only, fewer than its joins, and so shows joined.
    pub parts: Rc<[Rc<ParaStyle>]>,
}

impl ParaStyle {
    /// Layout equivalence: every resolved value, the style and the tracked property change, which the markup views
    /// draw. The authored properties ([`ParaStyle::direct`]) are not compared.
    pub fn layout_eq(&self, o: &ParaStyle) -> bool {
        let ParaStyle {
            style_id,
            jc,
            bidi,
            ind_left,
            ind_right,
            ind_first,
            space_before,
            space_after,
            before_auto,
            after_auto,
            line,
            line_rule,
            contextual,
            snap_to_grid,
            keep_next,
            keep_lines,
            page_break_before,
            widow_control,
            suppress_auto_hyphens,
            no_kinsoku,
            no_overflow_punct,
            no_auto_space_de,
            no_auto_space_dn,
            top_line_punct,
            no_word_wrap,
            tight_wrap,
            suppress_overlap,
            suppress_line_numbers,
            mirror_indents,
            div,
            div_boxes,
            tabs,
            border_top,
            border_bottom,
            border_left,
            border_right,
            border_between,
            shading,
            shd,
            frame,
            text_flow,
            in_frame,
            text_alignment,
            outline_lvl,
            collapsed,
            list_jc,
            num,
            list_fmt,
            list,
            mark_size,
            direct: _,
            change,
            joined,
            parts,
        } = self;
        *style_id == o.style_id
            && *jc == o.jc
            && *bidi == o.bidi
            && *ind_left == o.ind_left
            && *ind_right == o.ind_right
            && *ind_first == o.ind_first
            && *space_before == o.space_before
            && *space_after == o.space_after
            && *before_auto == o.before_auto
            && *after_auto == o.after_auto
            && *line == o.line
            && *line_rule == o.line_rule
            && *contextual == o.contextual
            && *snap_to_grid == o.snap_to_grid
            && *keep_next == o.keep_next
            && *keep_lines == o.keep_lines
            && *page_break_before == o.page_break_before
            && *widow_control == o.widow_control
            && *suppress_auto_hyphens == o.suppress_auto_hyphens
            && *no_kinsoku == o.no_kinsoku
            && *no_overflow_punct == o.no_overflow_punct
            && *no_auto_space_de == o.no_auto_space_de
            && *no_auto_space_dn == o.no_auto_space_dn
            && *top_line_punct == o.top_line_punct
            && *no_word_wrap == o.no_word_wrap
            && *tight_wrap == o.tight_wrap
            && *suppress_overlap == o.suppress_overlap
            && *suppress_line_numbers == o.suppress_line_numbers
            && *mirror_indents == o.mirror_indents
            && *div == o.div
            && *div_boxes == o.div_boxes
            && *tabs == o.tabs
            && *border_top == o.border_top
            && *border_bottom == o.border_bottom
            && *border_left == o.border_left
            && *border_right == o.border_right
            && *border_between == o.border_between
            && *shading == o.shading
            && *shd == o.shd
            && *frame == o.frame
            && *text_flow == o.text_flow
            && *in_frame == o.in_frame
            && *text_alignment == o.text_alignment
            && *outline_lvl == o.outline_lvl
            && *collapsed == o.collapsed
            && *list_jc == o.list_jc
            && *num == o.num
            && *list_fmt == o.list_fmt
            && *list == o.list
            && *mark_size == o.mark_size
            && *change == o.change
            && joined.len() == o.joined.len()
            && joined.iter().zip(o.joined.iter()).all(|(a, b)| a.0 == b.0 && a.1.layout_eq(&b.1))
            && parts.len() == o.parts.len()
            && parts.iter().zip(o.parts.iter()).all(|(a, b)| a.layout_eq(b))
    }

    /// Authored equality: the direct formatting, and the tracked property change with the formatting it records.
    pub fn authored_eq(&self, o: &ParaStyle) -> bool {
        self.direct.authored_eq(&o.direct)
            && match (&self.change, &o.change) {
                (None, None) => true,
                (Some(x), Some(y)) => {
                    x.mark == y.mark
                        && x.properties == y.properties
                        && (!x.properties || x.previous.direct.authored_eq(&y.previous.direct))
                        && x.numbering_change.0 == y.numbering_change.0
                }
                _ => false,
            }
    }
}

impl PartialEq for ParaStyle {
    fn eq(&self, o: &ParaStyle) -> bool {
        self.layout_eq(o)
    }
}

/// A field or content control: what the host names (`f12` / `c7` in JSON).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ElementId {
    /// A field.
    Field(FieldId),
    /// A content control.
    Control(ControlId),
}

/// Whether a field shows its result or its code (Alt+F9, Shift+F9; not saved).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FieldView {
    /// Its result.
    #[default]
    Result,
    /// Its code.
    Code,
}

/// What a field is, as layout evaluates it.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldKind {
    /// PAGE: the page number.
    Page,
    /// `w:pgNum`: a PAGE field whose code is written for it; in a header or footer its chapter number is its
    /// section's.
    PgNum,
    /// NUMPAGES: the page count.
    NumPages,
    /// SECTIONPAGES: the section's page count.
    SectionPages,
    /// SECTION: the section number.
    Section,
    /// PAGEREF: the page of a bookmark.
    PageRef(String),
    /// REF: a bookmark's text.
    Ref(String),
    /// SEQ: the next number of a sequence.
    Seq(String),
    /// STYLEREF: the text of the nearest paragraph in a style.
    StyleRef(String),
    /// DATE, TIME and the like: a date.
    Date,
    /// IF: a condition's result.
    If,
    /// A bookmark's name as the whole code: its text.
    Bookmark(String),
    /// A code whose first word can be neither a field name nor a bookmark (`"http://…"` left by a merge): Word shows
    /// nothing.
    Invalid,
    /// A sub-template's placeholder.
    SubTemplate,
    /// A legacy check box (FORMCHECKBOX): its box size in points (`None`: its run's size), and whether it is checked.
    CheckBox {
        /// Its box size in points; `None`: its run's size.
        size: Option<f64>,
        /// Checked.
        checked: bool,
    },
    /// A legacy drop-down (FORMDROPDOWN): it shows its entry as its value, a blank without one.
    DropDown,
    /// A document property, by DOCPROPERTY's name (AUTHOR, TITLE, SUBJECT, KEYWORDS, COMMENTS, LASTSAVEDBY and
    /// REVNUM read theirs).
    DocProperty(String),
    /// A document variable (DOCVARIABLE).
    DocVariable(String),
    /// A document date property (CREATEDATE, SAVEDATE, PRINTDATE): its core property's name.
    DocDate(&'static str),
    /// FILENAME: the name the host gave the document (the editor's `file_name`).
    FileName,
    /// NOTEREF: a bookmark's note, and the note its first note reference is (found when fields are evaluated).
    NoteRef(String, Option<(NoteKind, i64)>),
    /// LISTNUM: the next number of a list kept in fields.
    ListNum,
    /// AUTONUM: the count of AUTONUM fields to here.
    AutoNum,
    /// An `=` formula.
    Formula,
}

impl FieldKind {
    /// Whether the kind is not its code's: a legacy form field's comes from its form data, `w:pgNum`'s from its
    /// element.
    pub fn is_not_from_code(&self) -> bool {
        matches!(self, FieldKind::CheckBox { .. } | FieldKind::DropDown | FieldKind::PgNum)
    }
}

/// An inline picture or shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    /// Width, points.
    pub w: f64,
    /// Height, points.
    pub h: f64,
    /// A resource in [`Document::images`].
    pub image: Option<ResourceId>,
    /// Its alternative text (`wp:docPr/@descr`).
    pub alt: Rc<str>,
    /// Lock aspect ratio (`a:graphicFrameLocks/@noChangeAspect` or `a:picLocks/@noChangeAspect`): corner handles keep
    /// the ratio.
    pub lock_aspect: bool,
    /// The effect extents (left, top, right, bottom) around the box, points.
    pub effect: [f64; 4],
    /// The DrawingML object: its geometry, paint and transform (`None`: a picture inserted without them).
    pub shape: Option<Box<crate::graphic::Shape<Origin>>>,
    /// An inline shape's text (`wps:txbx`): its story, edited as a text box's (`StoryId::TextBox` of its id).
    pub text: Option<Box<InlineText>>,
    /// Where it was read from (its run child as read; editable reads only): a copy pasted into the part it came from is
    /// written from it.
    pub origin: Origin,
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

    /// The story this inline holds, to edit.
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
    /// The id its story goes by.
    pub id: u64,
    /// Its story.
    pub text_box: TextBox,
}

/// The most vertices a wrap polygon keeps: a longer one read or made is thinned to about this many.
pub const MAX_WRAP_POINTS: usize = 1024;

/// `pts` thinned to at most [`MAX_WRAP_POINTS`] vertices, the closing copy of the first aside, by keeping every n-th;
/// closed again when it was closed.
pub fn bounded_polygon(mut pts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let closed = pts.len() > 1 && pts.first() == pts.last();
    let distinct = pts.len() - usize::from(closed);
    if distinct <= MAX_WRAP_POINTS {
        return pts;
    }
    if closed {
        pts.pop();
    }
    let step = distinct.div_ceil(MAX_WRAP_POINTS);
    let mut k = 0;
    pts.retain(|_| {
        k += 1;
        (k - 1) % step == 0
    });
    if closed
        && let Some(&first) = pts.first()
        && pts.last() != Some(&first)
    {
        pts.push(first);
    }
    pts
}

/// An anchored (floating) picture or text box (the FLOAT rules).
#[derive(Clone, Debug, PartialEq)]
pub struct Float {
    /// Identity in layouts and editing ([`next_para_id`]'s counter): fragments name it, selections and drags keep it.
    pub id: u64,
    /// Width, points.
    pub w: f64,
    /// The width grows with the text (a text box sized to fit it).
    pub auto_w: bool,
    /// Height, points.
    pub h: f64,
    /// A resource in [`Document::images`].
    pub image: Option<ResourceId>,
    /// A picture's alternative text (`wp:docPr/@descr`).
    pub alt: Rc<str>,
    /// Lock aspect ratio, as for [`Picture::lock_aspect`].
    pub lock_aspect: bool,
    /// Text box content, laid out at the box's content width.
    pub text_box: Option<TextBox>,
    /// What the horizontal position is relative to.
    pub h_relative: HorizontalRelation,
    /// The horizontal alignment; `None`: placed by `h_offset`.
    pub h_align: Option<HorizontalAlign>,
    /// The horizontal offset, points.
    pub h_offset: f64,
    /// What the vertical position is relative to.
    pub v_relative: VerticalRelation,
    /// The vertical alignment; `None`: placed by `v_offset`.
    pub v_align: Option<VerticalAlign>,
    /// The vertical offset, points.
    pub v_offset: f64,
    /// How text wraps around it.
    pub wrap: FloatWrap,
    /// Which sides text wraps on.
    pub wrap_text: WrapText,
    /// The distance from the text (top, bottom, left, right), points.
    pub dist: [f64; 4],
    /// The effect extents (left, top, right, bottom) around the box, points.
    pub effect: [f64; 4],
    /// The wrap polygon of a tight or through float, as fractions of its box.
    pub poly: Option<Rc<[(f64, f64)]>>,
    /// The wrap polygon was edited (`wp:wrapPolygon/@edited`): it isn't made again from the object's outline.
    pub poly_edited: bool,
    /// Behind the text.
    pub behind: bool,
    /// Other floats may overlap it.
    pub allow_overlap: bool,
    /// Laid out in its table cell.
    pub layout_in_cell: bool,
    /// Document order of the float, for F13.
    pub order: usize,
    /// Its place in the drawing order.
    pub z: u32,
    /// Rotated a quarter turn: its width and height swap for wrapping and placing.
    pub rot_swap: bool,
    /// Read from a VML shape: its effect extents stand for its outline's pixels.
    pub vml: bool,
    /// Its size and position relative to the page or margins.
    pub relative: Relative,
    /// Where it was read from (its drawing, or a frame's paragraphs): saving drops that when the float is deleted.
    pub origin: Origin,
    /// `wp:anchor/@locked`: the anchor stays in its paragraph when the float is moved.
    pub locked: bool,
    /// Anchored in a footnote or endnote: laid out as an inline object at its anchor, wrapping nothing.
    pub in_note: bool,
    /// A group's pictures and text boxes.
    pub parts: Vec<GroupPart>,
    /// The DrawingML object (geometry, paint, transform, group tree); `None` for frames and floats made without one.
    pub shape: Option<Box<crate::graphic::Shape<Origin>>>,
}

/// Relative size and position (`wp14:sizeRelH`, `wp14:pctPosHOffset`, VML `mso-width-percent`, `mso-left-percent`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Relative {
    /// Width and height: the box they're relative to and the share in tenths of a percent.
    pub size: [Option<(String, i64)>; 2],
    /// Horizontal and vertical position, in tenths of a percent of the reference box.
    pub pos: [Option<i64>; 2],
    /// A picture whose box was sized: the size it's drawn at, scaled evenly into the box from its top left.
    pub drawn: Option<(f64, f64)>,
}

/// A picture or text box inside a group.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPart {
    /// Its box in the float, points.
    pub rect: [f64; 4],
    /// A resource in [`Document::images`].
    pub image: Option<ResourceId>,
    /// Its text.
    pub text_box: Option<TextBox>,
    /// The part's place in the float's shape tree.
    pub path: crate::graphic::ChildPath,
}

impl Float {
    /// The picture control a floating picture control holds: the float is a text box with no fill, no line and no
    /// insets whose story is one paragraph holding only that control's one picture.
    pub fn picture_control(&self, controls: &HashMap<ControlId, Control>) -> Option<ControlId> {
        let tb = self.text_box.as_ref()?;
        if tb.insets != [0.0; 4] || tb.outline != 0.0 || tb.blocks.len() != 1 {
            return None;
        }
        if let Some(crate::graphic::ShapeKind::Sp(sp)) = self.shape.as_ref().map(|s| &s.kind) {
            match &sp.fill {
                Some(crate::graphic::paint::Fill::None) => {}
                None if sp.style.is_none() => {}
                _ => return None,
            }
        }
        let Block::Para(p) = &tb.blocks[0] else { return None };
        let picture = |c: &ControlId| controls.get(c).is_some_and(|k| k.props.kind == ControlKind::Picture);
        let content: Vec<&Inline> = p
            .inlines
            .iter()
            .filter(|i| {
                !matches!(i, Inline::Foreign(..) | Inline::FieldEdge { .. } | Inline::Range(..)) && !matches!(i, Inline::Text(t, _) if t.is_empty())
            })
            .collect();
        match content.as_slice() {
            [Inline::ControlStart(a), Inline::Picture(..), Inline::ControlEnd(b)] if a == b && picture(a) => Some(*a),
            [Inline::Picture(..)] if p.controls.len() == 1 && picture(&p.controls[0]) => Some(p.controls[0]),
            _ => None,
        }
    }

    /// A hidden shape (`wp:docPr/@hidden`, VML `visibility:hidden`): not drawn, and nothing wraps it.
    pub fn hidden(&self) -> bool {
        self.shape.as_ref().is_some_and(|s| s.nv.hidden)
    }

    /// Whether it is laid out in its table cell under `compat`.
    pub fn in_cell(&self, compat: Compat) -> bool {
        compat == Compat::Generic
            || self.layout_in_cell
            || self.h_relative == HorizontalRelation::Character
            || self.v_relative == VerticalRelation::Line
    }
}

/// A text box's story and how it sits in its box.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBox {
    /// The story.
    pub blocks: Vec<Block>,
    /// The insets (left, top, right, bottom), points.
    pub insets: [f64; 4],
    /// The outline's width, points.
    pub outline: f64,
    /// The box grows to fit its text.
    pub auto_fit: bool,
    /// Where the text sits in the box: 0 top, 1 centre, 2 bottom.
    pub v_anchor: u8,
    /// A frame's text-flow code (`text_flow`).
    pub flow: u8,
}

impl TextBox {
    /// Equal but for the story's blocks.
    pub fn same_box(&self, o: &TextBox) -> bool {
        let TextBox { blocks: _, insets, outline, auto_fit, v_anchor, flow } = self;
        *insets == o.insets && *outline == o.outline && *auto_fit == o.auto_fit && *v_anchor == o.v_anchor && *flow == o.flow
    }
}

/// What a paragraph holds, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    /// Text.
    Text(String, Run),
    /// A tab.
    Tab(Run),
    /// `w:ptab`: a tab to an absolute position, not to the next stop.
    PTab(PTab, Run),
    /// A line, page or column break.
    Break(BreakKind, Run),
    /// `w:noBreakHyphen`, `w:softHyphen` are in the text as U+2011 / U+00AD; `w:sym` as its character.
    Sym {
        /// The character.
        ch: char,
        /// Its font.
        font: String,
        /// Its formatting.
        run: Run,
        /// The checkbox control it is the face of (`Document::controls`), if any.
        control: Option<ControlId>,
    },
    /// An inline picture or shape.
    Picture(Picture, Run),
    /// A floating picture or text box, anchored here.
    Float(Box<Float>, Run),
    /// A footnote or endnote reference.
    NoteRef {
        /// Footnote or endnote.
        kind: NoteKind,
        /// The note's id.
        id: i64,
        /// A custom mark follows in place of the number (`w:customMarkFollows`).
        custom: bool,
        /// Its formatting.
        run: Run,
    },
    /// `w:footnoteRef` / `w:endnoteRef` inside a note: the note's own number (`id` = the note's id).
    NoteMark(NoteKind, i64, Run),
    /// A note separator line.
    Separator {
        /// A continuation separator.
        continuation: bool,
        /// Drawn across the column in every mode (an endnote continuation separator).
        full: bool,
        /// Its formatting.
        run: Run,
    },
    /// A field's start (`w:fldChar begin`). The inlines up to its [`Inline::FieldSep`] (or its
    /// [`Inline::FieldEnd`] when it has none) are its code, those after the separator its result; a nested field is
    /// another marker triple inside either.
    FieldBegin {
        /// The field's id.
        id: FieldId,
        /// The kind layout evaluates, parsed from the code (the DOCX adapter's `fields::parse`); `None`: the result is shown.
        kind: Option<FieldKind>,
        /// `w:fldLock`, `w:dirty`.
        lock: bool,
        /// Its result is to be updated.
        dirty: bool,
        /// Showing its result or its code.
        view: FieldView,
        /// Template design: what an empty result shows; not saved.
        label: Option<Rc<str>>,
        /// The value its result shows, once evaluated.
        value: Option<String>,
        /// A legacy form field's data (`w:ffData`).
        form: Option<Rc<FormField>>,
        /// Its formatting.
        run: Run,
    },
    /// `w:fldChar separate`: the end of a field's code, the start of its result.
    FieldSep {
        /// The field's id.
        id: FieldId,
        /// Its formatting.
        run: Run,
    },
    /// `w:fldChar end`.
    FieldEnd {
        /// The field's id.
        id: FieldId,
        /// Its formatting.
        run: Run,
    },
    /// Where a comment's, a move's, a bookmark's or an editing permission's range starts or ends. No units.
    Range(RangeMark),
    /// A comment's reference mark (`w:commentReference`): where the comment is anchored. No units.
    CommentRef {
        /// The comment's id.
        id: CommentId,
        /// Its formatting.
        run: Run,
    },
    /// Where an inline content control's content starts and ends. No units: a caret beside one is
    /// beside the control's edge.
    ControlStart(ControlId),
    /// Where an inline content control's content ends.
    ControlEnd(ControlId),
    /// A ruby (`w:ruby`): one unit.
    Ruby(Rc<Ruby>, Run),
    /// A date block (`w:dayShort`, `w:dayLong`, `w:monthShort`, `w:monthLong`, `w:yearShort`, `w:yearLong`, by its
    /// element name) and the date it shows, filled in when fields are evaluated: one unit.
    DateBlock(&'static str, String, Run),
    /// An edge of a field whose result runs across paragraphs, in the run of its markers (their formatting and
    /// tracked changes): no units but a code's braces ([`FieldEdge::units`]).
    FieldEdge {
        /// The edge.
        edge: Rc<Edge>,
        /// Its formatting.
        run: Run,
    },
    /// Content the engine doesn't model, between runs (an equation, a subdocument): no units, nothing drawn, saved as
    /// read wherever it moves. No units.
    Foreign(Rc<Foreign>),
}

/// A legacy form field's data: what saving writes back.
#[derive(Clone, Debug, PartialEq)]
pub struct FormField {
    /// Its settings and value.
    pub data: crate::forms::FormData,
    /// Where it was read from (its `w:ffData` and data as read): written as read while `data` is.
    pub origin: Origin,
}

/// A field edge marker ([`Inline::FieldEdge`]): one allocation for each edge made, which its copies share.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    /// What edge it is.
    pub kind: FieldEdge,
    /// Where it was read from (its markers' runs as read; none for an edge made since, or one read from runs holding
    /// more than its markers).
    pub origin: Origin,
}

/// An edge of a field whose result runs across paragraphs (a table of contents): its start (its begin marker, code
/// and separator) or its end, each kept as the runs of its markers. A story's starts and ends pair as they nest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldEdge {
    /// Where the field begins: its result follows.
    Start {
        /// The field's code.
        code: String,
        /// Locked: updating leaves its result.
        lock: bool,
        /// Its result is to be updated (`w:dirty`).
        dirty: bool,
        /// Showing its code (Alt+F9, Shift+F9; not saved): the edge is the code's opening brace, the code runs follow
        /// up to a [`FieldEdge::Code`], and the result after it is hidden up to the end.
        view: FieldView,
    },
    /// The closing brace of a start's code shown as text: the code's runs are between the start and it.
    Code,
    /// The end of a paragraph whose mark a field showing its code hides: the result's paragraphs join the code's line.
    Fold,
    /// Where the field's result ends.
    End,
}

impl FieldEdge {
    /// The units the edge takes: a code's braces one each, other edges none.
    pub fn units(&self) -> usize {
        match self {
            FieldEdge::Start { view: FieldView::Code, .. } | FieldEdge::Code => 1,
            _ => 0,
        }
    }

    /// A new marker of this edge in `run`.
    pub fn inline(self, run: Run) -> Inline {
        Inline::FieldEdge { edge: Rc::new(Edge { kind: self, origin: Origin::NONE }), run }
    }
}

/// The date blocks' element names and the date pictures they show.
pub const DATE_BLOCKS: [(&str, &str); 6] =
    [("dayShort", "dd"), ("dayLong", "dddd"), ("monthShort", "MM"), ("monthLong", "MMMM"), ("yearShort", "yy"), ("yearLong", "yyyy")];

/// A ruby: base text with its annotation set above it, laid out as one object.
#[derive(Clone, Debug, PartialEq)]
pub struct Ruby {
    /// The base text, in runs.
    pub base: Vec<(String, Run)>,
    /// The annotation, in runs.
    pub text: Vec<(String, Run)>,
    /// `w:rubyAlign`: center, distributeLetter, distributeSpace, left, right, rightVertical (0 to 5).
    pub align: u8,
    /// `w:hpsRaise`: the annotation's baseline above the base's, points.
    pub raise: f64,
    /// `w:hps`: the annotation's size, points.
    pub size: Option<f64>,
    /// Where it was read from (its `w:ruby` element as read): what saving writes.
    pub origin: Origin,
}

/// A paragraph's list label.
#[derive(Clone, Debug, PartialEq)]
pub struct ListLabel {
    /// The label's text.
    pub text: String,
    /// The label's formatting.
    pub run: Run,
    /// Its alignment.
    pub jc: ListJc,
    /// What follows it: a tab, a space or nothing.
    pub suff: ListSuffix,
    /// A picture bullet's image (a resource in [`Document::images`]), drawn instead of the text.
    pub picture: Option<ResourceId>,
    /// A legacy level's space and indent (the DOCX adapter's `numbering::Level::legacy`).
    pub legacy: Option<(f64, f64)>,
}

/// A paragraph.
#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    /// Its effective formatting.
    pub style: Rc<ParaStyle>,
    /// Its mark's formatting.
    pub mark: Run,
    /// Its list label.
    pub list: Option<ListLabel>,
    /// What it holds.
    pub inlines: Vec<Inline>,
    /// Where the paragraph was read from (editable reads only; new paragraphs have none).
    pub origin: Origin,
    /// Identity across edits and layouts ([`next_para_id`]); a clone keeps it, so a copied or split-off paragraph
    /// needs a fresh one. Layout tags drawn glyphs with it, and incremental layout reuses pages by it.
    pub id: u64,
    /// The block-level content controls around it, outermost first: consecutive blocks with an id are
    /// that control's content.
    pub controls: Vec<ControlId>,
    /// What the HTML divisions starting and ending here add.
    pub div: Option<Rc<DivEdge>>,
}

impl Paragraph {
    /// The division edge built again over the paragraph's current style, after the style was replaced.
    pub fn refresh_div(&mut self) {
        if let Some(d) = self.div.as_ref().filter(|d| !Rc::ptr_eq(&d.from, &self.style)) {
            self.div = Some(Rc::new(DivEdge::new(&self.style, d.before, d.after, d.boxes.clone())));
        }
    }
}

/// One of the stories Word draws around a page's notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoteSeparator {
    /// The line above the notes.
    Separator,
    /// The line above notes going on from the page before.
    Continuation,
    /// The notice below notes going on to the next page.
    Notice,
}

impl NoteSeparator {
    /// Its key in [`Document::separators`] for notes of `kind`.
    pub fn key(self, kind: NoteKind) -> String {
        let tag = match kind {
            NoteKind::Foot => "footnote",
            NoteKind::End => "endnote",
        };
        let name = match self {
            NoteSeparator::Separator => "separator",
            NoteSeparator::Continuation => "continuationSeparator",
            NoteSeparator::Notice => "continuationNotice",
        };
        format!("{tag}:{name}")
    }
}

/// A custom XML part: opaque to the editor, kept with the document.
#[derive(Clone, Debug, PartialEq)]
pub struct DataPart {
    /// Its root element's namespace: how the host names it (Word's `CustomXMLParts.SelectByNamespace`).
    pub namespace: Option<String>,
    /// `ds:itemID`, unique in the package.
    pub item_id: String,
    /// Its XML.
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
    /// How the bodies' style and numbering ids map onto the document's.
    pub map: crate::glossary::GlossaryMap,
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
    /// The gallery it is in.
    pub gallery: Gallery,
    /// `w:guid`, as written.
    pub guid: Option<String>,
    /// `w:docPartBody`.
    pub body: Vec<Block>,
    /// Whether it is as read, added or removed.
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
    /// Any other value, as written.
    Other(String),
}

/// Whether a building block is as read, added since, or removed since (kept so undo can bring it back).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PartState {
    /// As read.
    #[default]
    AsRead,
    /// Added since reading.
    Added,
    /// Removed since reading.
    Removed,
}

/// A content control (`w:sdt`) or custom XML wrapper: its properties, and where it was read from (written back as it
/// was while its properties are unchanged).
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    /// Its properties.
    pub props: ControlProps,
    /// Where it was read from (its markup and properties as read; none for a new control).
    pub origin: Origin,
}

/// A content control kind the editor keeps as written (`ControlKind::Other`), as its kind element names it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum KeptKind {
    /// `w:docPartList`: a building block gallery, offering the building blocks of its gallery (by Word's name for it,
    /// `Quick Parts`, `Custom 1`, ...) and category (`None`: any).
    Gallery {
        /// The gallery.
        gallery: Option<String>,
        /// The category; `None`: any.
        category: Option<String>,
    },
    /// `w:docPartObj`: a building block inserted from a gallery (a cover page, a table of contents); `unique`:
    /// `w:docPartUnique`, one of its kind in the document.
    BuildingBlock {
        /// The gallery.
        gallery: Option<String>,
        /// The category.
        category: Option<String>,
        /// One of its kind in the document.
        unique: bool,
    },
    /// `w:citation`.
    Citation,
    /// `w:bibliography`.
    Bibliography,
    /// `w:equation`.
    Equation,
}

/// A drop-down's or combo box's entry (`w:listItem`), its attributes as given.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ListEntry {
    /// `w:displayText`: what it shows.
    pub text: Option<String>,
    /// `w:value`: what choosing it stores.
    pub value: Option<String>,
}

impl ControlProps {
    /// The markup it keeps as read (its kind element and other children), to declare the namespaces it uses where it
    /// is written.
    pub fn kept_xml_mut(&mut self) -> impl Iterator<Item = &mut String> {
        self.kind_xml.iter_mut().map(|k| &mut k.xml).chain(self.rest.iter_mut().map(|(_, x)| x))
    }

    /// A building block gallery (`w:docPartList`): `Some` of its category (`None`: any).
    pub fn gallery(&self) -> Option<Option<String>> {
        match &self.kept {
            Some(KeptKind::Gallery { category, .. }) => Some(category.clone()),
            _ => None,
        }
    }
}

impl Control {
    /// A new control (no markup read).
    pub fn new(props: ControlProps) -> Control {
        Control { props, origin: Origin::NONE }
    }
}

/// A content control's properties (`w:sdtPr`), parsed where they are read. The settings the
/// editor never changes are kept as raw XML, written back in schema order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ControlProps {
    /// What kind of control it is.
    pub kind: ControlKind,
    /// `w:tag`.
    pub tag: Option<String>,
    /// `w:alias`: the control's title.
    pub alias: Option<String>,
    /// What the user may not do to it.
    pub lock: Lock,
    /// `w:showingPlcHdr`.
    pub showing_placeholder: bool,
    /// `w:placeholder`: the name of the building block it shows while empty (its `w:docPart`).
    pub placeholder: Option<String>,
    /// `w:dataBinding`: the custom XML node it shows and writes.
    pub binding: Option<ControlBinding>,
    /// The kind's element as read (`w:date` with its children, `w:dropDownList`, ...), with the kind it was read as:
    /// written back while the control's kind is that one, a drop-down's and a kept kind's while their typed settings
    /// are as read.
    pub kind_xml: Option<KindXml>,
    /// A drop-down's entries.
    pub items: Vec<ListEntry>,
    /// A drop-down is a combo box (`w:comboBox`, else `w:dropDownList`): its entries suggest, any text goes.
    pub combo: bool,
    /// What a control the editor keeps as written is (`ControlKind::Other`).
    pub kept: Option<KeptKind>,
    /// `w:temporary`: the control goes when its content is edited, leaving the content.
    pub temporary: bool,
    /// `w15:appearance`: how it is drawn (`None`: not given, or a value this doesn't know).
    pub appearance: Option<ControlAppearance>,
    /// `w15:color`: `Some` of its colour (`0xRRGGBB`) when given, `Some(None)` for one this doesn't read (`auto`).
    pub color: Option<Option<u32>>,
    /// `w:rPr`: the formatting its content takes.
    pub run: Option<ControlRun>,
    /// `w:id`: its number (`None`: none, or one this doesn't read).
    pub id: Option<i64>,
    /// Every other `w:sdtPr` child (and `w:rPr`, `w:id`, `w:placeholder`, `w:temporary`, `w:dataBinding`,
    /// `w15:appearance` and `w15:color` as read), by local name, in reading order.
    pub rest: Vec<(String, String)>,
}

/// A content control's data binding (`w:dataBinding`), its attributes as given.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ControlBinding {
    /// `w:xpath`: the node, an XPath (`None`: not given, so not evaluated).
    pub xpath: Option<String>,
    /// `w:prefixMappings`: the XPath's namespace prefixes (`xmlns:p='uri'` pairs).
    pub prefix_mappings: Option<String>,
    /// `w:storeItemID`: the data part's item id.
    pub store_item_id: Option<String>,
}

/// A content control's `w15:appearance`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAppearance {
    /// `boundingBox`: a box around it.
    BoundingBox,
    /// `tags`: start and end tags.
    Tags,
    /// `hidden`: nothing.
    Hidden,
}

/// A content control's run properties (`w:sdtPr/w:rPr`): typed, with the `w:rPr` they are written as.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlRun {
    /// The properties.
    pub props: Rc<crate::format::CharProps>,
    /// Their `w:rPr`.
    pub xml: Rc<str>,
}

/// A content control's kind element as read, and the kind it was read as.
#[derive(Clone, Debug, PartialEq)]
pub struct KindXml {
    /// The element as read.
    pub xml: String,
    /// The kind read from it.
    pub kind: ControlKind,
}

/// What a date control stores in its data (`w:storeMappedDataAs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DateStorage {
    /// The text it shows.
    Text,
    /// The date (`xsd:date`).
    Date,
    /// The date and time (`xsd:dateTime`), Word's default.
    DateTime,
}

/// `w:lock`: what the user may not do to a control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lock {
    /// No lock.
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
    /// Whether the control can be deleted.
    pub fn deletable(self) -> bool {
        matches!(self, Lock::None | Lock::ContentLocked)
    }

    /// Whether its content can be edited.
    pub fn editable(self) -> bool {
        matches!(self, Lock::None | Lock::ControlLocked)
    }
}

/// What a content control is.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum ControlKind {
    /// `w:text`.
    PlainText {
        /// It takes line breaks.
        multi_line: bool,
    },
    /// `w:richText`, or no kind element (Word's default).
    RichText,
    /// `w:picture`.
    Picture,
    /// `w14:checkbox`: the characters it shows checked and unchecked, their fonts (`w14:font`), and whether its markup
    /// says it is checked (`w14:checked`; the face it shows says what it is).
    Checkbox {
        /// The character shown checked.
        checked: char,
        /// The character shown unchecked.
        unchecked: char,
        /// The checked character's font.
        checked_font: Option<String>,
        /// The unchecked character's font.
        unchecked_font: Option<String>,
        /// Whether its markup says it is checked.
        value: Option<bool>,
    },
    /// `w:date`: its display format and language, its calendar (`w:calendar`), what it stores in its data
    /// (`w:storeMappedDataAs`; `None`: not given, or a value this doesn't know) and the date it shows (`w:fullDate`,
    /// `YYYY-MM-DDTHH:MM:SSZ`).
    Date {
        /// The display format.
        format: String,
        /// The display language.
        lid: Option<String>,
        /// The calendar.
        calendar: Option<String>,
        /// What it stores in its data.
        storage: Option<DateStorage>,
        /// The date it shows.
        full_date: Option<String>,
    },
    /// `w:dropDownList` or `w:comboBox` (its entries in `ControlProps::items`).
    DropDown,
    /// `w:group`.
    Group,
    /// `w15:repeatingSection`: its title (`w15:sectionTitle`), and whether its items are fixed
    /// (`w15:doNotAllowInsertDeleteSection`: none added or removed).
    RepeatingSection {
        /// Its title.
        title: Option<String>,
        /// Its items are fixed.
        fixed: bool,
    },
    /// `w15:repeatingSectionItem`: one item (usually a row) of a repeating section.
    RepeatingItem,
    /// A kind the editor doesn't know, or a `w:customXml` wrapper.
    #[default]
    Other,
}

/// A cell's place in a vertical merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VMerge {
    /// Not merged.
    None,
    /// Starts a merge.
    Restart,
    /// Continues the merge above.
    Continue,
}

/// A `w:textDirection` value (transitional or strict spelling) as its text-flow code: `lrTb` 0, `tbRl` 1, `btLr` 3,
/// `lrTbV` 4, `tbRlV` 5, `tbLrV` 7; anything else is horizontal.
pub fn text_flow(v: &str) -> u8 {
    match v {
        "tbRl" | "rl" => 1,
        "btLr" | "lr" => 3,
        "lrTbV" | "tbV" => 4,
        "tbRlV" | "rlV" => 5,
        "tbLrV" | "lrV" => 7,
        _ => 0,
    }
}

/// A table cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Its width.
    pub width: Option<Width>,
    /// The grid columns it spans.
    pub span: usize,
    /// Its place in a vertical merge.
    pub v_merge: VMerge,
    /// Top border.
    pub top: Option<Border>,
    /// Left border.
    pub left: Option<Border>,
    /// Bottom border.
    pub bottom: Option<Border>,
    /// Right border.
    pub right: Option<Border>,
    /// The shading's resolved colour (`0xRRGGBB`).
    pub shading: Option<u32>,
    /// The shading's stripe or cross, drawn over [`Cell::shading`].
    pub hatch: Option<crate::format::Hatch>,
    /// Margins (top, left, bottom, right), points.
    pub margins: [f64; 4],
    /// Vertical alignment.
    pub v_align: VerticalAlignment,
    /// `w:textDirection`, as written.
    pub text_direction: String,
    /// Its text doesn't wrap.
    pub no_wrap: bool,
    /// Its text is fitted to its width.
    pub fit_text: bool,
    /// `w:hideMark`.
    pub hide_mark: bool,
    /// The diagonal borders, top left to bottom right and top right to bottom left (`w:tl2br`, `w:tr2bl`).
    pub diagonals: Option<Rc<[Option<Border>; 2]>>,
    /// The cell's authored properties (its `w:tcPr` as read, or as table formatting changed it): what re-resolution
    /// merges over the cell's table-style region when its place in the table changes.
    pub direct: crate::format::CellDirect,
    /// Its content.
    pub blocks: Vec<Block>,
    /// The cell-level controls around it, outermost first ([`Document::controls`]).
    pub controls: Vec<ControlId>,
    /// Tracked insertion, deletion or merge of the cell, and a tracked change of its properties.
    pub rev: CellRevisions,
    /// Its identifier and the header cells it is under, by theirs (`w:tc/@w:id`, `tcPr/w:headers`).
    pub ids: Option<Rc<CellIds>>,
}

/// A table cell's identifier (`w:tc/@w:id`) and the identifiers of the header cells that head it
/// (`tcPr/w:headers/w:header/@w:val`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellIds {
    /// Its identifier.
    pub id: Option<String>,
    /// The identifiers of the header cells that head it.
    pub headers: Vec<String>,
}

/// A table row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Its height and how it applies.
    pub height: Option<(f64, HeightRule)>,
    /// It can't break across pages.
    pub cant_split: bool,
    /// A header row, repeated on each page.
    pub header: bool,
    /// Grid columns before its first cell.
    pub grid_before: usize,
    /// Grid columns after its last cell.
    pub grid_after: usize,
    /// The width before its first cell.
    pub w_before: Option<Width>,
    /// The width after its last cell.
    pub w_after: Option<Width>,
    /// Its indent, points, when its exceptions give one.
    pub ind: Option<f64>,
    /// Its alignment, when its exceptions give one.
    pub jc: Option<Jc>,
    /// The HTML division the row is in (`trPr/w:divId`).
    pub div: Option<Rc<RowDiv>>,
    /// The table borders the row's exceptions (`w:tblPrEx`) give it, in [`Table::borders`] order; `None` for the
    /// table's.
    pub borders: Option<Rc<[Option<Border>; 6]>>,
    /// The row's exception shading (`w:tblPrEx/w:shd`).
    pub shading: Option<u32>,
    /// Its stripe or cross, drawn over [`Row::shading`].
    pub hatch: Option<crate::format::Hatch>,
    /// A row read from a later table joined to this one (nothing between them): that table's shading, fill and
    /// hatch, in place of [`Table::shd`]; none for a row whose exceptions' shading has no colour.
    pub table_shading: Option<(Option<u32>, Option<crate::format::Hatch>)>,
    /// The row-end mark is hidden (`trPr/w:hidden`).
    pub hidden: bool,
    /// Every character of the row, its row-end mark included, is hidden.
    pub vanished: bool,
    /// The row's own cell spacing (`trPr` or `w:tblPrEx`).
    pub cell_spacing: Option<f64>,
    /// The table width of the layout run the row starts or continues, when its exceptions fix its layout.
    pub run: Option<Width>,
    /// Compat ≤ 14, a table joined from adjacent tables whose rows' left cell margins differ by more than 5 twips:
    /// every column edge sits 5 twips right; `Some(true)` keeps the row's left edge (its first cell that much wider).
    pub nudge: Option<bool>,
    /// Its cells.
    pub cells: Vec<Cell>,
    /// The controls wrapping the row, outermost first.
    pub controls: Vec<ControlId>,
    /// Where the row was read from (editable reads only): saving keeps a row it still has. A copied row needs none, or
    /// it would be written as the row it was copied from.
    pub origin: Origin,
    /// The row's authored properties and its exceptions to its table's (its `w:trPr` and `w:tblPrEx`; editable reads
    /// only): what a rewritten row's properties are written from.
    pub direct: crate::format::RowDirect,
    /// Its exceptions to its table's properties (`w:tblPrEx`), as [`Row::direct`].
    pub exceptions: crate::format::ExceptionsDirect,
    /// Tracked insertion or deletion of the row, and tracked changes of its properties.
    pub rev: RowRevisions,
}

/// A table's alternative text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableAlt {
    /// Title.
    pub title: String,
    /// Description.
    pub description: String,
}

/// A table.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    /// The table style.
    pub style_id: Option<String>,
    /// Its preferred width.
    pub width: Width,
    /// Alignment.
    pub jc: Jc,
    /// The table's own `w:jc`; a layout run after the first row falls back to it.
    pub own_jc: Option<Jc>,
    /// Indent from the margin, points.
    pub ind: f64,
    /// A `pct` indent, the fraction of the width the table is laid out in; [`Table::ind`] is then 0.
    pub ind_pct: Option<f64>,
    /// The indent was set, by the table or its style.
    pub ind_set: bool,
    /// Borders: top, left, bottom, right, inside horizontal, inside vertical.
    pub borders: [Option<Border>; 6],
    /// Fixed layout: no autofit.
    pub fixed: bool,
    /// Cell spacing, points.
    pub cell_spacing: f64,
    /// The table's shading (`w:tblPr/w:shd`, its own or its style's), drawn around spaced cells.
    pub shd: Option<crate::format::Shd>,
    /// Which table style regions apply.
    pub look: Option<Look>,
    /// Rows and columns in a band of the style's banding.
    pub bands: (i64, i64),
    /// The alternative text's title and description (`w:tblCaption`, `w:tblDescription`).
    pub alt: Option<Rc<TableAlt>>,
    /// Its position, when it floats.
    pub floating: Option<TablePos>,
    /// Right to left.
    pub bidi_visual: bool,
    /// The grid's column widths, points.
    pub grid: Vec<f64>,
    /// Its rows.
    pub rows: Vec<Row>,
    /// Where the table was read from (editable reads only).
    pub origin: Origin,
    /// Identity in layouts ([`next_para_id`]): drawn rows name their table (table handles, border dragging).
    pub id: u64,
    /// The table's authored properties (its `w:tblPr` as read): what re-resolution merges over a new table style's
    /// properties.
    pub direct: crate::format::TableDirect,
    /// The block-level content controls around it, outermost first.
    pub controls: Vec<ControlId>,
    /// A tracked change of the table's properties, and of its column grid.
    pub change: Option<Rc<TableChange>>,
    /// A tracked change of its column grid.
    pub grid_change: Option<Rc<GridChange>>,
}

impl Cell {
    /// Authored equality of the cell's own state: its width, span and merge, its authored properties, its revisions
    /// and its ids. Its blocks are compared as blocks.
    pub fn authored_eq(&self, o: &Cell) -> bool {
        self.width == o.width
            && self.span == o.span
            && self.v_merge == o.v_merge
            && self.direct.authored_eq(&o.direct)
            && self.rev == o.rev
            && self.ids == o.ids
    }
}

impl Row {
    /// Authored equality of the row's own state: its grid offsets, authored properties and exceptions, its revisions
    /// and its cells' own state. Resolved values (height, header, can't split) are not compared.
    pub fn authored_eq(&self, o: &Row) -> bool {
        (self.grid_before, self.grid_after) == (o.grid_before, o.grid_after)
            && self.direct.authored_eq(&o.direct)
            && self.exceptions.authored_eq(&o.exceptions)
            && self.rev == o.rev
            && self.cells.len() == o.cells.len()
            && self.cells.iter().zip(&o.cells).all(|(c, d)| c.authored_eq(d))
    }
}

impl Table {
    /// Authored equality of the table's own state: its authored properties, look, position, alternative text, grid
    /// and tracked changes. Resolved values (style, width, alignment, indent, layout, spacing) and rows are not
    /// compared.
    pub fn authored_eq(&self, o: &Table) -> bool {
        self.direct.authored_eq(&o.direct)
            && self.look == o.look
            && self.floating == o.floating
            && self.alt == o.alt
            && self.grid == o.grid
            && (&self.change, &self.grid_change) == (&o.change, &o.grid_change)
    }
}

/// A block of a story.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant, reason = "paragraphs dominate; boxing them costs an allocation each")]
pub enum Block {
    /// A paragraph.
    Para(Paragraph),
    /// A table.
    Table(Table),
}

impl Block {
    /// A copy with ids of its own (its paragraphs', tables', floats' and inline texts', inside it too) and no origin
    /// for its paragraphs, tables and rows: a copied subtree, saved as new. Floats and pictures keep theirs.
    pub fn fresh_identity(&self) -> Block {
        match self {
            Block::Para(p) => Block::Para(p.fresh_identity()),
            Block::Table(t) => Block::Table(t.fresh_identity()),
        }
    }
}

impl Paragraph {
    /// A copy with ids of its own and no origin, as [`Block::fresh_identity`].
    pub fn fresh_identity(&self) -> Paragraph {
        let mut p = self.clone();
        p.renew_identity();
        for i in &mut p.inlines {
            let story = match i {
                Inline::Float(f, _) => {
                    f.id = next_para_id();
                    f.text_box.as_mut()
                }
                Inline::Picture(pic, _) => pic.text.as_mut().map(|t| {
                    t.id = next_para_id();
                    &mut t.text_box
                }),
                _ => None,
            };
            if let Some(tb) = story {
                tb.blocks = tb.blocks.iter().map(Block::fresh_identity).collect();
            }
        }
        p
    }

    /// A new id and no origin for it alone: what it holds keeps theirs.
    pub fn renew_identity(&mut self) {
        self.id = next_para_id();
        self.detach();
    }

    /// No origin: saved as a new paragraph.
    pub fn detach(&mut self) {
        self.origin = Origin::NONE;
    }
}

impl Table {
    /// A copy with ids of its own and no origin, its rows' and cells' blocks' too, as [`Block::fresh_identity`].
    pub fn fresh_identity(&self) -> Table {
        let mut t = self.clone();
        t.renew_identity();
        for r in &mut t.rows {
            r.detach();
            for c in &mut r.cells {
                c.blocks = c.blocks.iter().map(Block::fresh_identity).collect();
            }
        }
        t
    }

    /// A new id and no origin for it alone: its rows keep theirs.
    pub fn renew_identity(&mut self) {
        self.id = next_para_id();
        self.detach();
    }

    /// No origin: saved as a new table.
    pub fn detach(&mut self) {
        self.origin = Origin::NONE;
    }
}

impl Row {
    /// No origin: saved as a new row.
    pub fn detach(&mut self) {
        self.origin = Origin::NONE;
    }
}

/// How a section starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionBreak {
    /// On a new page.
    NextPage,
    /// On the same page.
    Continuous,
    /// On the next even page.
    EvenPage,
    /// On the next odd page.
    OddPage,
    /// In the next column.
    NextColumn,
}

/// A section's settings the Page Setup, Layout and header and footer commands change: size, margins, columns, a
/// different first page, borders, its break, page and line numbering, vertical alignment, text direction and notes.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionPage {
    /// Page width, points.
    pub page_w: f64,
    /// Page height, points.
    pub page_h: f64,
    /// Top margin, points.
    pub top: f64,
    /// Bottom margin, points.
    pub bottom: f64,
    /// Left margin, points.
    pub left: f64,
    /// Right margin, points.
    pub right: f64,
    /// The header's distance from the page's top, points.
    pub header: f64,
    /// The footer's distance from the page's bottom, points.
    pub footer: f64,
    /// Gutter, points.
    pub gutter: f64,
    /// Text columns.
    pub columns: Columns,
    /// A different first page.
    pub title_pg: bool,
    /// Page borders.
    pub page_borders: Option<Rc<PageBorders>>,
    /// The authored `w:pgBorders`: typed (what isn't modelled, art borders' ids, kept beside), with the element as
    /// written while they are the borders read from it.
    pub page_borders_direct: crate::format::PageBordersDirect,
    /// How it starts.
    pub break_kind: SectionBreak,
    /// The page number it starts at.
    pub pg_start: Option<i64>,
    /// The page number format, as written.
    pub pg_fmt: Option<String>,
    /// Chapter numbers in page numbers: the heading level and the separator.
    pub chapter: Option<(i64, String)>,
    /// Line numbering.
    pub line_numbers: Option<LineNumbering>,
    /// Vertical alignment of the text on the page.
    pub v_align: VerticalAlignment,
    /// The text-flow code (`text_flow`).
    pub text_flow: u8,
    /// Footnote and endnote properties (foot, end).
    pub notes: [NoteProps; 2],
    /// The section's endnotes wait for the next section's.
    pub no_endnote: bool,
    /// `w15:footnoteColumns` (0 when absent).
    pub footnote_columns: u32,
    /// `w:formProt`: filling-in-forms protection covers the section (Word writes it only when off).
    pub form_protected: bool,
    /// Its headers and footers (default, first, even); `None`: the section before's.
    pub headers: HfRefs,
    /// Its footers, as [`SectionPage::headers`].
    pub footers: HfRefs,
}

/// A section's page size, margins, columns and different first page, borrowed from it or its tracked change: what
/// layout reads per page.
#[derive(Clone, Copy, Debug)]
pub struct PageSetup<'a> {
    /// Page width, points.
    pub page_w: f64,
    /// Page height, points.
    pub page_h: f64,
    /// Top margin, points.
    pub top: f64,
    /// Bottom margin, points.
    pub bottom: f64,
    /// Left margin, points.
    pub left: f64,
    /// Right margin, points.
    pub right: f64,
    /// The header's distance from the page's top, points.
    pub header: f64,
    /// The footer's distance from the page's bottom, points.
    pub footer: f64,
    /// Gutter, points.
    pub gutter: f64,
    /// Text columns.
    pub columns: &'a Columns,
    /// A different first page.
    pub title_pg: bool,
}

impl<'a> From<&'a SectionPage> for PageSetup<'a> {
    fn from(p: &'a SectionPage) -> PageSetup<'a> {
        let SectionPage { page_w, page_h, top, bottom, left, right, header, footer, gutter, title_pg, .. } = *p;
        PageSetup { page_w, page_h, top, bottom, left, right, header, footer, gutter, columns: &p.columns, title_pg }
    }
}

impl<'a> From<&'a Section> for PageSetup<'a> {
    fn from(s: &'a Section) -> PageSetup<'a> {
        let Section { page_w, page_h, top, bottom, left, right, header, footer, gutter, title_pg, .. } = *s;
        PageSetup { page_w, page_h, top, bottom, left, right, header, footer, gutter, columns: &s.columns, title_pg }
    }
}

impl Section {
    /// Its page settings.
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
            page_borders_direct: self.page_borders_direct.clone(),
            break_kind: self.break_kind,
            pg_start: self.pg_start,
            pg_fmt: self.pg_fmt.clone(),
            chapter: self.chapter.clone(),
            line_numbers: self.line_numbers.clone(),
            v_align: self.v_align,
            text_flow: self.text_flow,
            notes: self.notes.clone(),
            no_endnote: self.no_endnote,
            footnote_columns: self.footnote_columns,
            form_protected: self.form_protected,
            headers: self.headers,
            footers: self.footers,
        }
    }

    /// The section's settings without its blocks, read page or tracked change: a new section made from it.
    pub fn without_blocks(&self) -> Section {
        Section { change: None, ..self.settings() }
    }

    /// The section's settings and tracked change, without its blocks or read page.
    pub fn settings(&self) -> Section {
        Section {
            break_kind: self.break_kind,
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
            headers: self.headers,
            footers: self.footers,
            pg_start: self.pg_start,
            pg_fmt: self.pg_fmt.clone(),
            chapter: self.chapter.clone(),
            line_numbers: self.line_numbers.clone(),
            v_align: self.v_align,
            page_borders: self.page_borders.clone(),
            page_borders_direct: self.page_borders_direct.clone(),
            line_grid: self.line_grid,
            notes: self.notes.clone(),
            no_endnote: self.no_endnote,
            text_flow: self.text_flow,
            rtl_gutter: self.rtl_gutter,
            footnote_columns: self.footnote_columns,
            form_protected: self.form_protected,
            blocks: Vec::new(),
            change: self.change.clone(),
            origin: Origin::NONE,
        }
    }

    /// Gives it the page settings `p`.
    pub fn set_page(&mut self, p: SectionPage) {
        (self.page_w, self.page_h, self.top, self.bottom, self.left, self.right) = (p.page_w, p.page_h, p.top, p.bottom, p.left, p.right);
        (self.header, self.footer, self.gutter, self.columns, self.title_pg) = (p.header, p.footer, p.gutter, p.columns, p.title_pg);
        (self.page_borders, self.page_borders_direct) = (p.page_borders, p.page_borders_direct);
        (self.break_kind, self.pg_start, self.pg_fmt, self.chapter, self.line_numbers) =
            (p.break_kind, p.pg_start, p.pg_fmt, p.chapter, p.line_numbers);
        (self.v_align, self.text_flow, self.notes, self.no_endnote) = (p.v_align, p.text_flow, p.notes, p.no_endnote);
        (self.footnote_columns, self.form_protected) = (p.footnote_columns, p.form_protected);
        (self.headers, self.footers) = (p.headers, p.footers);
    }
}

impl SectionPage {
    /// Authored equality: the same settings and the same authored page borders.
    pub fn authored_eq(&self, o: &SectionPage) -> bool {
        self == o && self.page_borders_direct.authored_eq(&o.page_borders_direct)
    }
}

impl Section {
    /// Authored equality: the same settings, authored page borders, tracked change and blocks.
    pub fn authored_eq(&self, o: &Section) -> bool {
        self == o
            && self.page_borders_direct.authored_eq(&o.page_borders_direct)
            && match (&self.change, &o.change) {
                (Some(a), Some(b)) => a.authored_eq(b),
                (a, b) => a.is_none() && b.is_none(),
            }
    }
}

/// A section's text columns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Columns {
    /// Column widths and the space after each (explicit for unequal columns).
    pub cols: Vec<(f64, f64)>,
    /// The columns are of equal width.
    pub equal: bool,
    /// How many.
    pub num: usize,
    /// The space between equal columns, points.
    pub space: f64,
    /// A line between columns.
    pub separator: bool,
}

impl Columns {
    /// The most text columns a section has, as in Word.
    pub const MAX: usize = 45;
}

/// A section's line numbering.
#[derive(Clone, Debug, PartialEq)]
pub struct LineNumbering {
    /// Numbers every this many lines.
    pub count_by: i64,
    /// The first number.
    pub start: i64,
    /// The distance from the text, points.
    pub distance: f64,
    /// When the numbering starts again.
    pub restart: LineNumberingRestart,
}

/// A section's `w:footnotePr` / `w:endnotePr`: number format, start and restart; for footnotes, `w:pos beneathText`.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteProps {
    /// The number format, as written.
    pub fmt: Option<String>,
    /// The first number.
    pub start: i64,
    /// When the numbering starts again.
    pub restart: NoteRestart,
    /// Footnotes go beneath the text rather than at the page's foot.
    pub beneath: bool,
}

impl Default for NoteProps {
    fn default() -> NoteProps {
        NoteProps { fmt: None, start: 1, restart: NoteRestart::Continuous, beneath: false }
    }
}

/// When note numbering starts again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoteRestart {
    /// Never.
    #[default]
    Continuous,
    /// Each section.
    EachSect,
    /// Each page.
    EachPage,
}

/// A section's page borders, resolved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageBorders {
    /// Top.
    pub top: Option<Border>,
    /// Left.
    pub left: Option<Border>,
    /// Bottom.
    pub bottom: Option<Border>,
    /// Right.
    pub right: Option<Border>,
    /// Measured from the page's edge, not the text.
    pub offset_from_page: bool,
    /// The pages they show on.
    pub display: PageBorderDisplay,
    /// They surround the header.
    pub surround_header: bool,
    /// They surround the footer.
    pub surround_footer: bool,
    /// Behind the text and objects they meet (`w:zOrder="back"`); else in front of them.
    pub behind_text: bool,
}

/// Header/footer references per variant: default, first, even.
pub type HfRefs = [Option<HeaderFooterId>; 3];

/// A section: its settings and its blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// How it starts.
    pub break_kind: SectionBreak,
    /// Page width, points.
    pub page_w: f64,
    /// Page height, points.
    pub page_h: f64,
    /// Top margin, points.
    pub top: f64,
    /// Bottom margin, points.
    pub bottom: f64,
    /// Left margin, points.
    pub left: f64,
    /// Right margin, points.
    pub right: f64,
    /// The header's distance from the page's top, points.
    pub header: f64,
    /// The footer's distance from the page's bottom, points.
    pub footer: f64,
    /// Gutter, points.
    pub gutter: f64,
    /// Text columns.
    pub columns: Columns,
    /// A different first page.
    pub title_pg: bool,
    /// Its headers (default, first, even); `None`: the section before's.
    pub headers: HfRefs,
    /// Its footers, as [`Section::headers`].
    pub footers: HfRefs,
    /// The page number it starts at.
    pub pg_start: Option<i64>,
    /// The page number format, as written.
    pub pg_fmt: Option<String>,
    /// `w:pgNumType w:chapStyle` / `w:chapSep`: page numbers lead with the number of the last heading of this level,
    /// then this separator.
    pub chapter: Option<(i64, String)>,
    /// Line numbering.
    pub line_numbers: Option<LineNumbering>,
    /// Vertical alignment of the text on the page.
    pub v_align: VerticalAlignment,
    /// Page borders.
    pub page_borders: Option<Rc<PageBorders>>,
    /// The authored `w:pgBorders`, typed, with the element as written while they are the borders read from it.
    pub page_borders_direct: crate::format::PageBordersDirect,
    /// The document grid's line pitch, in 4096ths of a point.
    pub line_grid: Option<i32>,
    /// Footnote and endnote properties, by [`NoteKind`] (foot, end).
    pub notes: [NoteProps; 2],
    /// `w:noEndnote`: the section's endnotes wait for the next section's.
    pub no_endnote: bool,
    /// The `w:textDirection` text-flow code (`text_flow`; a section's `tbLrV` reads as `tbRlV`).
    pub text_flow: u8,
    /// `w:rtlGutter`: the gutter is on the right.
    pub rtl_gutter: bool,
    /// `w15:footnoteColumns`: the footnote area's column count (0 when absent).
    pub footnote_columns: u32,
    /// `w:formProt`: filling-in-forms protection covers the section.
    pub form_protected: bool,
    /// Its content.
    pub blocks: Vec<Block>,
    /// A tracked change of the section's properties.
    pub change: Option<Rc<SectionChange>>,
    /// Where it was read from (its page as read; editable reads only): saving rewrites only the page settings that
    /// changed.
    pub origin: Origin,
}

/// The document's settings.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    /// Word's compatibility mode and options.
    pub policy: LayoutPolicy,
    /// The default tab stop interval, points.
    pub default_tab: f64,
    /// `w:characterSpacingControl` is compressPunctuation or compressPunctuationAndJapaneseKana.
    pub compress_punctuation: bool,
    /// Different odd and even headers and footers.
    pub even_and_odd: bool,
    /// Mirrored margins.
    pub mirror_margins: bool,
    /// The gutter is at the top.
    pub gutter_at_top: bool,
    /// The style new tables take (`w:defaultTableStyle`).
    pub default_table_style: Option<String>,
    /// Legacy form fields aren't shaded (`w:doNotShadeFormData`).
    pub no_form_shading: bool,
    /// AutoFormat as you type may format text the formatting restrictions keep formatting from
    /// (`w:autoFormatOverride`).
    pub auto_format_override: bool,
    /// Formatting restrictions keep the theme and its colours and fonts from changing (`w:styleLockTheme`).
    pub style_lock_theme: bool,
    /// Formatting restrictions keep the Quick Style Set from changing (`w:styleLockQFSet`).
    pub style_lock_style_set: bool,
    /// The document variables (`w:docVars`), in order.
    pub variables: Vec<(String, String)>,
    /// `w:decimalSymbol` and `w:listSeparator`: what `=` formulas read numbers and arguments with.
    pub decimal_symbol: Option<char>,
    /// The list separator.
    pub list_separator: Option<char>,
    /// `w:bookFoldPrinting` / `w:bookFoldRevPrinting`: `Some(reversed)`.
    pub book_fold: Option<bool>,
    /// `w:strictFirstAndLastChars`: the strict East Asian line-breaking level.
    pub strict_kinsoku: bool,
    /// Automatic hyphenation.
    pub auto_hyphenation: bool,
    /// The hyphenation zone, points.
    pub hyphenation_zone: f64,
    /// The most consecutive hyphenated lines; 0 for any.
    pub consecutive_hyphen_limit: i64,
    /// Words in capitals aren't hyphenated.
    pub do_not_hyphenate_caps: bool,
    /// Page borders don't surround the header.
    pub borders_do_not_surround_header: bool,
    /// Page borders don't surround the footer.
    pub borders_do_not_surround_footer: bool,
    /// `w:trackRevisions`: edits are recorded as revisions.
    pub track_revisions: bool,
    /// `w:doNotTrackMoves`: moved text is recorded as an insertion and a deletion, not a move.
    pub do_not_track_moves: bool,
    /// `w:doNotTrackFormatting`: formatting changes made while tracking are not recorded.
    pub do_not_track_formatting: bool,
    /// `w:endnotePr/w:pos sectEnd`: endnotes follow their section rather than the document.
    pub endnotes_sect_end: bool,
    /// The note stories `w:footnotePr` / `w:endnotePr` list, as `footnote:<id>` / `endnote:<id>`.
    pub listed_notes: Vec<String>,
    /// Document protection.
    pub protection: Option<Protection>,
    /// `w:revisionView` as written, kept for saving.
    pub revision_view_raw: Raw,
    /// `w:revisionView`: the markup the document shows when it opens.
    pub revision_view: crate::review::ShowMarkup,
    /// `w:doNotDisplayPageBoundaries`: the white space between pages is hidden when the document opens.
    pub hides_page_boundaries: bool,
    /// `w:displayBackgroundShape`: the page background shows (Word shows none without it).
    pub display_background_shape: bool,
}

impl Document {
    /// The page background as it shows: its colour and fill, both `None` when Word doesn't display it.
    pub fn shown_background(&self) -> (Option<u32>, Option<&crate::graphic::paint::Fill>) {
        match self.settings.display_background_shape {
            true => (self.page_colour, self.page_fill.as_ref()),
            false => (None, None),
        }
    }
}

impl Settings {
    /// The compatibility mode layout follows.
    pub fn compat(&self) -> Compat {
        self.policy.compat()
    }
}

/// Content a document imports in place (`w:altChunk`): Word reads it as content when it opens the document.
#[derive(Clone, Debug)]
pub struct AltChunk {
    /// The empty paragraph standing in its place ([`Paragraph::id`]), whose source is the `w:altChunk` element.
    pub para: u64,
    /// What it imports.
    pub content: AltContent,
}

/// What a `w:altChunk` imports, shared by every one importing the same part.
#[derive(Clone, Debug)]
pub enum AltContent {
    /// HTML.
    Html(Rc<str>),
    /// Plain text: a paragraph a line.
    Text(Rc<str>),
    /// A document, read: its body comes in, in this document's styles unless `match_src` (`w:matchSrc`).
    Document {
        /// The document.
        doc: Rc<Document>,
        /// It keeps its own formatting.
        match_src: bool,
    },
}

/// A document: the layout engine's input.
#[derive(Clone, Debug, Default)]
pub struct Document {
    /// Its sections.
    pub sections: Vec<Section>,
    /// Its theme.
    pub theme: Rc<crate::graphic::theme::Theme>,
    /// How the theme's colours map onto the colour scheme.
    pub colour_map: crate::graphic::theme::ColourMap,
    /// The page background's colour (`w:background`), a theme colour's tint or shade applied.
    pub page_colour: Option<u32>,
    /// The page background's fill over its colour (`v:background`): a gradient, pattern or picture.
    pub page_fill: Option<crate::graphic::paint::Fill>,
    /// The theme colour `page_colour` is, when it is one.
    pub page_colour_theme: Option<ThemeColourRef>,
    /// Headers and footers, in id order.
    pub stories: std::collections::BTreeMap<HeaderFooterId, Vec<Block>>,
    /// Footnotes, by id.
    pub footnotes: HashMap<i64, Vec<Block>>,
    /// Endnotes, by id.
    pub endnotes: HashMap<i64, Vec<Block>>,
    /// Footnote separator / continuation separator stories, when the document defines them.
    pub separators: HashMap<String, Vec<Block>>,
    /// Images, by id.
    pub images: Resources,
    /// HTML imported in place (`w:altChunk`), each standing as an empty paragraph until the editor brings it in.
    pub alt_chunks: Vec<AltChunk>,
    /// The core, extended and custom document properties.
    pub properties: crate::properties::Properties,
    /// Content controls the model keeps: row-level wrappers (repeating sections and their items) and checkboxes,
    /// by id.
    pub controls: HashMap<ControlId, Control>,
    /// The package's custom XML parts: definition data the host keeps with the template.
    pub data_parts: Vec<DataPart>,
    /// The glossary part's building blocks, when the package has one.
    pub glossary: Option<Glossary>,
    /// Placeholders filled since reading, by image: drawn as their image, saved without `w:showingPlcHdr`.
    pub filled: std::collections::HashSet<ResourceId>,
    /// Pictures that can't be deleted, by image: those in picture controls that can't be deleted (`w:lock`
    /// `sdtLocked` or `sdtContentLocked`).
    pub undeletable: std::collections::HashSet<ResourceId>,
    /// Pictures in picture controls, by image: the innermost picture control of each picture showing it.
    pub controlled: HashMap<ResourceId, Vec<ControlId>>,
    /// Settings.
    pub settings: Settings,
    /// The formatting line numbers take (the Line Number character style's).
    pub line_number_run: Option<Run>,
    /// The styles, numbering and theme it resolves with. `Rc`: clipboard fragments carry their source
    /// document's resolver to resolve pasted formatting.
    pub definitions: Option<Rc<crate::definitions::Definitions>>,
    /// The font table's description of each family, by lowercase name: what a resolver is told about a missing one.
    pub font_descriptions: HashMap<String, crate::fonts::FontDescription>,
    /// Who made the document's revisions and comments.
    pub people: People,
    /// Comments.
    pub comments: Comments,
    /// New revision, comment and paragraph ids, past those read.
    pub review_ids: IdCounter,
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

    /// Each picture in a picture control, by image, with its innermost picture control.
    pub fn picture_controls(&self) -> Vec<(ResourceId, ControlId)> {
        if !self.controls.values().any(|c| c.props.kind == ControlKind::Picture) {
            return Vec::new();
        }
        type Found = Vec<(ResourceId, ControlId)>;
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
    /// Left, centre or right.
    pub align: TabKind,
    /// Relative to the text area, else to the paragraph's indents.
    pub margin: bool,
    /// The leader.
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
            | Inline::Ruby(_, r)
            | Inline::DateBlock(_, _, r) => Some(r),
            Inline::Sym { run, .. }
            | Inline::NoteRef { run, .. }
            | Inline::Separator { run, .. }
            | Inline::FieldBegin { run, .. }
            | Inline::FieldSep { run, .. }
            | Inline::FieldEnd { run, .. }
            | Inline::CommentRef { run, .. } => Some(run),
            // A field edge's run is its markers', not text's.
            Inline::Range(_) | Inline::ControlStart(_) | Inline::ControlEnd(_) | Inline::FieldEdge { .. } | Inline::Foreign(_) => None,
        }
    }

    /// A bookmark marker's name, and whether it starts the bookmark.
    pub fn bookmark(&self) -> Option<(&str, bool)> {
        match self {
            Inline::Range(r) => r.bookmark(),
            _ => None,
        }
    }

    /// An editing permission marker: whether it starts the permission's range, and the permission's id.
    pub fn permission(&self) -> Option<(bool, i64)> {
        match self {
            Inline::Range(r) => r.permission(),
            _ => None,
        }
    }

    /// Caret units the inline occupies: a character per character of text, one per other drawn inline, none for
    /// bookmarks, range markers, comment references, anchored objects and foreign content.
    pub fn units(&self) -> usize {
        match self {
            Inline::Text(s, _) => s.chars().count(),
            Inline::Range(_) | Inline::CommentRef { .. } | Inline::Float(..) | Inline::ControlStart(_) | Inline::ControlEnd(_) => 0,
            Inline::FieldEdge { edge, .. } => edge.kind.units(),
            Inline::Foreign(_) => 0,
            _ => 1,
        }
    }
}
