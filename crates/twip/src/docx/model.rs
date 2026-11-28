//! The layout engine's input: a document with every style resolved (the STYLE rules). Lengths are points.

pub use crate::document::anchor::{FloatWrap, HorizontalAlign, HorizontalRelation, VerticalAlign, VerticalRelation, WrapText};
pub use crate::document::content::{BreakKind, NoteKind};
pub use crate::document::foreign::{Foreign, FormatId};
pub use crate::document::ident::{ControlId, FieldId, HeaderFooterId, ResourceId, next_para_id};
pub use crate::document::line::Underline;
pub use crate::document::origin::Origin;
pub use crate::document::paragraph::{ListId, ListJc, ListRef, ListSuffix, TextAlignment};
pub use crate::document::policy::{Compat, LayoutPolicy};
pub use crate::document::resource::Resources;
pub use crate::document::section::{HeightRule, LineNumberingRestart, PageBorderDisplay, VerticalAlignment};
use crate::docx::char_props::RunXml;

use super::font_table::GenericFamily;
use std::collections::HashMap;
use std::rc::Rc;

use super::props::{Border, FramePr, Jc, Leader, LineRule, Look, Tab, TabKind, TablePos, VertAlign, Width};
use super::review::{
    CellRevisions, CommentId, Comments, GridChange, IdCounter, ParaChange, People, Protection, RangeMark, RowRevisions, RunRevisions, SectionChange,
    TableChange,
};

/// Effective character formatting of a run. Its `==` is [`RunStyle::layout_eq`].
#[derive(Clone, Debug)]
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
    /// `w:noProof`: no automatic hyphenation either.
    pub no_proof: bool,
    pub underline: Option<Underline>,
    /// The underline's own colour (`w:u/@w:color`); `None`: the text's.
    pub underline_color: Option<u32>,
    /// The theme colour `underline_color` is.
    pub underline_color_theme: Option<ThemeColourRef>,
    /// Emboss, engrave (`w:imprint`), outline and shadow.
    pub emboss: bool,
    pub imprint: bool,
    pub outline: bool,
    pub shadow: bool,
    /// The OpenType features as set, whatever the compatibility mode makes of them ([`RunStyle::ligatures`] and
    /// [`RunStyle::figures`] are what layout applies).
    pub typography: Typography,
    /// Word 2010's text effects through the style chain; `None` without any.
    pub effects: Option<Rc<TextEffects>>,
    pub color: Option<u32>,
    /// The theme colour `color` is.
    pub color_theme: Option<ThemeColourRef>,
    /// The theme font the Latin text's family is (`w:asciiTheme`), when it is one.
    pub font_theme: Option<ThemeFont>,
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
    /// OpenType number and style features asked for: 1 `lnum`, 2 `onum`, 4 `pnum`, 8 `tnum`, and `16 << (n − 1)` for
    /// `ssNN`.
    pub figures: u32,
    /// `w:fitText`: the width its region of text is fitted to (points) and the region's id.
    pub fit_text: Option<(f64, i64)>,
    /// `w:em`, when it marks the text.
    pub em: Option<super::props::Emphasis>,
    /// In a table of contents' result: the Hyperlink character style doesn't show ([`crate::docx::run_xml::rerun`]).
    pub toc: bool,
    pub lang: String,
    /// The East Asian language (`w:lang w:eastAsia`).
    pub lang_east_asia: String,
    pub style_id: Option<String>,
    /// The authored properties, with the `w:rPr` they were read as (editable reads only): what saving writes back
    /// for new runs in this formatting.
    pub direct: super::char_props::RunDirect,
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
    pub contextual_alternates: bool,
    /// 1 lining, 2 old style, 0 the font's default.
    pub number_form: u32,
    /// 4 proportional, 8 tabular, 0 the font's default.
    pub number_spacing: u32,
    /// Bit n − 1 for stylistic set n.
    pub stylistic_sets: u32,
}

/// A run's Word 2010 text effects (`w14:textFill`, `w14:textOutline`, `w14:shadow`, `w14:glow`, `w14:reflection`),
/// each as DrawingML reads it. Each is set or not on its own, and a style chain's levels override one at a time.
/// `w14:scene3d` and `w14:props3d` stay in the source XML: they are kept, and drawn flat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEffects {
    /// The letters' fill in place of the text colour: none, a solid or a gradient.
    pub fill: Option<crate::drawing::paint::Fill>,
    pub outline: Option<crate::drawing::paint::Line>,
    pub shadow: Option<crate::drawing::paint::OuterShadow>,
    pub glow: Option<crate::drawing::paint::Glow>,
    pub reflection: Option<crate::drawing::paint::Reflection>,
}

impl TextEffects {
    pub fn is_empty(&self) -> bool {
        *self == TextEffects::default()
    }

    /// The effects that show: without those written as off (a glow of no radius, a shadow or reflection of no size,
    /// an outline with no fill), which is how a run turns off its style's. `None` when none shows.
    pub fn shown(e: &Rc<TextEffects>) -> Option<Rc<TextEffects>> {
        use crate::drawing::paint::Fill;
        let glow = e.glow.as_ref().is_some_and(|g| g.radius.0 == 0);
        let shadow = e.shadow.as_ref().is_some_and(|s| s.sx.0 == 0 || s.sy.0 == 0);
        let reflection = e.reflection.as_ref().is_some_and(|r| r.sx.0 == 0 || r.sy.0 == 0);
        let outline = e.outline.as_ref().is_some_and(|l| matches!(l.fill, None | Some(Fill::None)));
        let shown = if glow || shadow || reflection || outline {
            let mut s = (**e).clone();
            if glow {
                s.glow = None;
            }
            if shadow {
                s.shadow = None;
            }
            if reflection {
                s.reflection = None;
            }
            if outline {
                s.outline = None;
            }
            Rc::new(s)
        } else {
            e.clone()
        };
        (!shown.is_empty()).then_some(shown)
    }

    /// Each effect `o` sets, over this one's.
    pub fn merge(&mut self, o: &TextEffects) {
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        take!(fill, outline, shadow, glow, reflection);
    }
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
    pub id: i64,
    pub shift: f64,
    pub lead: f64,
    pub trail: f64,
    pub boxes: Rc<[DivBox]>,
}

/// An HTML division (`w:div` in `webSettings`).
#[derive(Clone, Debug, PartialEq)]
pub struct Division {
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
    pub before: f64,
    pub after: f64,
    pub boxes: Rc<[DivBox]>,
    from: Rc<ParaStyle>,
    style: Rc<ParaStyle>,
}

impl DivEdge {
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
    pub right: f64,
    /// The box opens above this paragraph, closes below it.
    pub starts: bool,
    pub ends: bool,
}

/// Effective paragraph formatting. Its `==` is [`ParaStyle::layout_eq`].
#[derive(Clone, Debug)]
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
    /// `w:kinsoku` off: no East Asian line-breaking classes.
    pub no_kinsoku: bool,
    /// `w:overflowPunct` off: no punctuation hangs past the right margin.
    pub no_overflow_punct: bool,
    /// `w:autoSpaceDE` / `w:autoSpaceDN` off: no extra space between East Asian text and Latin letters / digits.
    pub no_auto_space_de: bool,
    pub no_auto_space_dn: bool,
    /// `w:topLinePunct`: an opening bracket starting a line in an East Asian paragraph loses its left half.
    pub top_line_punct: bool,
    /// `w:wordWrap` off: East Asian and Latin text may break between any two characters.
    pub no_word_wrap: bool,
    /// `w:textboxTightWrap` in a text box: 0 none, 1 all lines, 2 first and last, 3 first only, 4 last only.
    pub tight_wrap: u8,
    /// `w:suppressOverlap`: the paragraph's frame may not overlap other frames and floats.
    pub suppress_overlap: bool,
    pub suppress_line_numbers: bool,
    /// `w:mirrorIndents`: on mirrored pages the indents swap sides.
    pub mirror_indents: bool,
    /// The HTML division the paragraph is in (`w:divId`).
    pub div: Option<i64>,
    /// The bordered divisions around the paragraph, outermost first (only in [`Paragraph::para_style_in`]'s style:
    /// they come from the neighbours, [`DivEdge`]).
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
    /// The `w:textDirection` text-flow code (`text_flow`): a frame's paragraphs run in it.
    pub text_flow: u8,
    /// The paragraph is in a frame: its `frame` made the float that holds it.
    pub in_frame: bool,
    pub text_alignment: TextAlignment,
    pub outline_lvl: Option<i64>,
    /// `w15:collapsed`: the heading opens collapsed.
    pub collapsed: bool,
    pub list_jc: ListJc,
    /// The list the paragraph is in (direct or from its style), and that level's `w:numFmt`.
    pub num: Option<ListRef>,
    pub list_fmt: Option<String>,
    pub list: Option<i64>,
    pub mark_size: Option<f64>,
    /// The authored properties (editable reads only), what formatting edits change and saving writes back.
    pub direct: super::para_props::ParaDirect,
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
    /// `w:pgNum`: a PAGE field whose code is written for it; in a header or footer its chapter number is its
    /// section's.
    PgNum,
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
    /// A legacy check box (FORMCHECKBOX): its box size in points (`None`: its run's size), and whether it is checked.
    CheckBox {
        size: Option<f64>,
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
    /// FILENAME: the name the host gave the document ([`Document::file_name`]).
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

#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub w: f64,
    pub h: f64,
    /// A resource in [`Document::images`].
    pub image: Option<ResourceId>,
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
    /// Where it was read from (its run child as read; editable reads only): a copy pasted into the part it came from is
    /// written from it.
    pub origin: Origin,
}

impl Picture {
    /// Equal but for the text story's blocks.
    pub fn same_but_story(&self, o: &Picture) -> bool {
        let Picture { w, h, image, alt, lock_aspect, effect, shape, text, origin: _ } = self;
        let text = match (text, &o.text) {
            (Some(a), Some(b)) => a.id == b.id && a.text_box.same_box(&b.text_box),
            (a, b) => a.is_none() && b.is_none(),
        };
        text && *w == o.w
            && *h == o.h
            && *image == o.image
            && *alt == o.alt
            && *lock_aspect == o.lock_aspect
            && *effect == o.effect
            && *shape == o.shape
    }
}

impl Float {
    /// Equal but for the text box story's blocks.
    pub fn same_but_story(&self, o: &Float) -> bool {
        let Float {
            id,
            w,
            auto_w,
            h,
            image,
            alt,
            lock_aspect,
            text_box,
            h_relative,
            h_align,
            h_offset,
            v_relative,
            v_align,
            v_offset,
            wrap,
            wrap_text,
            dist,
            effect,
            poly,
            poly_edited,
            behind,
            allow_overlap,
            layout_in_cell,
            order,
            z,
            rot_swap,
            vml,
            relative,
            origin: _,
            locked,
            in_note,
            parts,
            shape,
        } = self;
        same_box(text_box.as_ref(), o.text_box.as_ref())
            && *id == o.id
            && *w == o.w
            && *auto_w == o.auto_w
            && *h == o.h
            && *image == o.image
            && *alt == o.alt
            && *lock_aspect == o.lock_aspect
            && *h_relative == o.h_relative
            && *h_align == o.h_align
            && *h_offset == o.h_offset
            && *v_relative == o.v_relative
            && *v_align == o.v_align
            && *v_offset == o.v_offset
            && *wrap == o.wrap
            && *wrap_text == o.wrap_text
            && *dist == o.dist
            && *effect == o.effect
            && *poly == o.poly
            && *poly_edited == o.poly_edited
            && *behind == o.behind
            && *allow_overlap == o.allow_overlap
            && *layout_in_cell == o.layout_in_cell
            && *order == o.order
            && *z == o.z
            && *rot_swap == o.rot_swap
            && *vml == o.vml
            && *relative == o.relative
            && self.src() == o.src()
            && *locked == o.locked
            && self.frame_src() == o.frame_src()
            && *in_note == o.in_note
            && *parts == o.parts
            && *shape == o.shape
    }

    /// A frame (`w:framePr` paragraphs made a float): read from one, or made since.
    pub fn is_frame(&self) -> bool {
        self.read_as_frame() || self.text_box.as_ref().is_some_and(|tb| tb.blocks.iter().any(|b| matches!(b, Block::Para(p) if p.style.in_frame)))
    }
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
    pub w: f64,
    pub auto_w: bool,
    pub h: f64,
    pub image: Option<ResourceId>,
    /// A picture's alternative text (`wp:docPr/@descr`).
    pub alt: Rc<str>,
    /// Lock aspect ratio, as for [`Picture::lock_aspect`].
    pub lock_aspect: bool,
    /// Text box content, laid out at the box's content width.
    pub text_box: Option<TextBox>,
    pub h_relative: HorizontalRelation,
    pub h_align: Option<HorizontalAlign>,
    pub h_offset: f64,
    pub v_relative: VerticalRelation,
    pub v_align: Option<VerticalAlign>,
    pub v_offset: f64,
    pub wrap: FloatWrap,
    pub wrap_text: WrapText,
    pub dist: [f64; 4],
    pub effect: [f64; 4],
    /// The wrap polygon of a tight or through float, as fractions of its box.
    pub poly: Option<Rc<[(f64, f64)]>>,
    /// The wrap polygon was edited (`wp:wrapPolygon/@edited`): it isn't made again from the object's outline.
    pub poly_edited: bool,
    pub behind: bool,
    pub allow_overlap: bool,
    pub layout_in_cell: bool,
    /// Document order of the float, for F13.
    pub order: usize,
    pub z: u32,
    pub rot_swap: bool,
    /// Read from a VML shape: its effect extents stand for its outline's pixels.
    pub vml: bool,
    pub relative: Relative,
    /// Where it was read from (its drawing, or a frame's paragraphs): saving drops that when the float is deleted.
    pub origin: Origin,
    /// `wp:anchor/@locked`: the anchor stays in its paragraph when the float is moved.
    pub locked: bool,
    /// Anchored in a footnote or endnote: laid out as an inline object at its anchor, wrapping nothing.
    pub in_note: bool,
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
    /// A picture whose box was sized: the size it's drawn at, scaled evenly into the box from its top left.
    pub drawn: Option<(f64, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupPart {
    pub rect: [f64; 4],
    pub image: Option<ResourceId>,
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

    pub fn in_cell(&self, compat: Compat) -> bool {
        compat == Compat::Generic
            || self.layout_in_cell
            || self.h_relative == HorizontalRelation::Character
            || self.v_relative == VerticalRelation::Line
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBox {
    pub blocks: Vec<Block>,
    pub insets: [f64; 4],
    pub outline: f64,
    pub auto_fit: bool,
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

fn same_box(a: Option<&TextBox>, b: Option<&TextBox>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.same_box(b),
        (a, b) => a.is_none() && b.is_none(),
    }
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
        /// Drawn across the column in every mode (an endnote continuation separator).
        full: bool,
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
        /// A legacy form field's data (`w:ffData`).
        form: Option<Rc<FormField>>,
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
    /// Where a comment's, a move's, a bookmark's or an editing permission's range starts or ends. No units.
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
    /// A date block (`w:dayShort`, `w:dayLong`, `w:monthShort`, `w:monthLong`, `w:yearShort`, `w:yearLong`, by its
    /// element name) and the date it shows, filled in when fields are evaluated: one unit.
    DateBlock(&'static str, String, Run),
    /// An edge of a field whose result runs across paragraphs, in the run of its markers (their formatting and
    /// tracked changes): no units but a code's braces ([`FieldEdge::units`]).
    FieldEdge {
        edge: Rc<Edge>,
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
    pub data: super::forms::FormData,
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
    pub base: Vec<(String, Run)>,
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

#[derive(Clone, Debug, PartialEq)]
pub struct ListLabel {
    pub text: String,
    pub run: Run,
    pub jc: ListJc,
    pub suff: ListSuffix,
    /// A picture bullet's image (a resource in [`Document::images`]), drawn instead of the text.
    pub picture: Option<ResourceId>,
    /// A legacy level's space and indent ([`super::numbering::Level::legacy`]).
    pub legacy: Option<(f64, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    pub style: Rc<ParaStyle>,
    pub mark: Run,
    pub list: Option<ListLabel>,
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

/// A paragraph joined onto the next one's start through its vanished mark, as read ([`Paragraph::joined_parts`]).
#[derive(Clone, Debug)]
pub struct JoinedPart {
    /// The unit its content ends at in the joined paragraph.
    pub end: u32,
    /// Its authored properties.
    pub ppr: super::para_props::ParaDirect,
    /// Its mark is a style separator (`w:specVanish`).
    pub spec_vanish: bool,
}

/// The package an editable document was read from ([`super::read_editable`]).
pub struct Source {
    pub package: Vec<u8>,
    /// The main part's name and XML as read.
    pub main: String,
    pub xml: String,
    /// Each section's blocks as read (what saving compares the edited document with).
    pub blocks: Vec<Vec<Block>>,
    /// Each header's and footer's part read (as [`Document::stories`]): its XML and its blocks as read.
    pub stories: HashMap<HeaderFooterId, (String, Vec<Block>)>,
    /// The part name of each header and footer the sections referred to as read, those that didn't load among them.
    pub story_parts: HashMap<HeaderFooterId, String>,
    /// Each read image's part name, by its id in [`Document::images`] (what a filled placeholder's image replaces).
    pub image_parts: HashMap<ResourceId, String>,
    /// The footnotes and endnotes parts: name, XML, and whether it holds footnotes.
    pub notes: Vec<(String, String, bool)>,
    /// Each note's blocks as read, by id.
    pub footnotes: HashMap<i64, Vec<Block>>,
    pub endnotes: HashMap<i64, Vec<Block>>,
    /// The content controls as read: saving takes the markup of those no longer in the document out.
    pub controls: HashMap<ControlId, Control>,
    /// The glossary part as read, when the package has one.
    pub glossary: Option<GlossarySource>,
    /// The page background's colour and fill as read.
    pub page_background: (Option<u32>, Option<crate::drawing::paint::Fill>),
    /// The page background colour's theme colour as read.
    pub page_theme: Option<ThemeColourRef>,
    /// The note separator stories as read (as [`Document::separators`], Word's built-in ones among them).
    pub separators: HashMap<String, Vec<Block>>,
    /// Word's built-in separator and continuation separator of each kind of note, by [`NoteSeparator::key`].
    pub separator_defaults: HashMap<String, Vec<Block>>,
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
    /// Its note `w:type`.
    pub fn xml_type(self) -> &'static str {
        match self {
            NoteSeparator::Separator => "separator",
            NoteSeparator::Continuation => "continuationSeparator",
            NoteSeparator::Notice => "continuationNotice",
        }
    }

    /// Its key in [`Document::separators`] for notes of `kind`.
    pub fn key(self, kind: NoteKind) -> String {
        let tag = match kind {
            NoteKind::Foot => "footnote",
            NoteKind::End => "endnote",
        };
        format!("{tag}:{}", self.xml_type())
    }
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
    /// Whether a gallery control naming gallery `name` (as Word's Content Control Properties name it: `Quick
    /// Parts`, `AutoText`, `Cover Pages`, ..., `Custom 1`) offers building blocks of this gallery: its built-in and
    /// custom ones, `Custom 1` to `Custom 5` their own.
    pub fn named(&self, name: &str) -> bool {
        let v = self.xml();
        let base = v.strip_prefix("cust").filter(|_| !matches!(self, Gallery::Custom(_))).map_or(v.as_str(), |b| b);
        let want: &[&str] = match name.trim() {
            "Quick Parts" => &["docParts", "QuickParts"],
            "AutoText" => &["autoTxt", "AutoTxt"],
            "Cover Pages" => &["coverPg", "CoverPg"],
            "Equations" => &["eq", "Eq"],
            "Footers" => &["ftrs", "Ftrs"],
            "Headers" => &["hdrs", "Hdrs"],
            "Page Numbers" => &["pgNum", "PgNum"],
            "Page Numbers (Top of Page)" => &["pgNumT", "PgNumT"],
            "Page Numbers (Bottom of Page)" => &["pgNumB", "PgNumB"],
            "Page Numbers (Margins)" => &["pgNumMargins", "PgNumMargins"],
            "Tables" => &["tbls", "Tbls"],
            "Table of Contents" => &["tblOfContents", "TblOfContents"],
            "Text Boxes" => &["txtBox", "TxtBox"],
            "Watermarks" => &["watermarks", "Watermarks"],
            "Bibliographies" => &["bib", "Bib"],
            "" => return true,
            other => return other.strip_prefix("Custom ").and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| *self == Gallery::Custom(n)),
        };
        want.contains(&base)
    }

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

/// A content control (`w:sdt`) or custom XML wrapper: its properties, and where it was read from (written back as it
/// was while its properties are unchanged).
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub props: ControlProps,
    /// Where it was read from (its markup and properties as read; none for a new control).
    pub origin: Origin,
}

/// A content control kind the editor keeps as written (`ControlKind::Other`), as its kind element names it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum KeptKind {
    /// `w:docPartList`: a building block gallery, offering the building blocks of its gallery (by Word's name for it,
    /// `Quick Parts`, `Custom 1`, ...) and category (`None`: any).
    Gallery { gallery: Option<String>, category: Option<String> },
    /// `w:docPartObj`: a building block inserted from a gallery (a cover page, a table of contents); `unique`:
    /// `w:docPartUnique`, one of its kind in the document.
    BuildingBlock { gallery: Option<String>, category: Option<String>, unique: bool },
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

    /// Whether it is written with the markup it was read with.
    pub fn as_read(&self) -> bool {
        !self.open().is_empty() && self.read() == Some(&self.props)
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

impl ControlAppearance {
    /// The appearance a `w15:appearance` value names; `None` for one this doesn't know.
    pub fn parse(v: &str) -> Option<ControlAppearance> {
        match v {
            "boundingBox" => Some(ControlAppearance::BoundingBox),
            "tags" => Some(ControlAppearance::Tags),
            "hidden" => Some(ControlAppearance::Hidden),
            _ => None,
        }
    }

    /// The `w15:appearance` value.
    pub fn xml(self) -> &'static str {
        match self {
            ControlAppearance::BoundingBox => "boundingBox",
            ControlAppearance::Tags => "tags",
            ControlAppearance::Hidden => "hidden",
        }
    }
}

/// A content control's run properties (`w:sdtPr/w:rPr`): typed, with the `w:rPr` they are written as.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlRun {
    /// The properties.
    pub props: Rc<crate::format::CharProps>,
    /// Their `w:rPr`.
    pub xml: Rc<str>,
}

impl ControlRun {
    /// As a run's authored properties, written as their `w:rPr`.
    pub fn direct(&self) -> super::char_props::RunDirect {
        super::char_props::RunDirect::shared(self.props.clone(), self.xml.clone())
    }
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

impl DateStorage {
    /// The storage a `w:storeMappedDataAs` value names; `None` for one this doesn't know.
    pub fn parse(v: &str) -> Option<DateStorage> {
        match v {
            "text" => Some(DateStorage::Text),
            "date" => Some(DateStorage::Date),
            "dateTime" => Some(DateStorage::DateTime),
            _ => None,
        }
    }

    /// The `w:storeMappedDataAs` value.
    pub fn xml(self) -> &'static str {
        match self {
            DateStorage::Text => "text",
            DateStorage::Date => "date",
            DateStorage::DateTime => "dateTime",
        }
    }
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum ControlKind {
    /// `w:text`.
    PlainText { multi_line: bool },
    /// `w:richText`, or no kind element (Word's default).
    RichText,
    /// `w:picture`.
    Picture,
    /// `w14:checkbox`: the characters it shows checked and unchecked, their fonts (`w14:font`), and whether its markup
    /// says it is checked (`w14:checked`; the face it shows says what it is).
    Checkbox { checked: char, unchecked: char, checked_font: Option<String>, unchecked_font: Option<String>, value: Option<bool> },
    /// `w:date`: its display format and language, its calendar (`w:calendar`), what it stores in its data
    /// (`w:storeMappedDataAs`; `None`: not given, or a value this doesn't know) and the date it shows (`w:fullDate`,
    /// `YYYY-MM-DDTHH:MM:SSZ`).
    Date { format: String, lid: Option<String>, calendar: Option<String>, storage: Option<DateStorage>, full_date: Option<String> },
    /// `w:dropDownList` or `w:comboBox` (its entries in `ControlProps::items`).
    DropDown,
    /// `w:group`.
    Group,
    /// `w15:repeatingSection`: its title (`w15:sectionTitle`), and whether its items are fixed
    /// (`w15:doNotAllowInsertDeleteSection`: none added or removed).
    RepeatingSection { title: Option<String>, fixed: bool },
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
    /// The shading's stripe or cross, drawn over [`Cell::shading`].
    pub hatch: Option<super::props::Hatch>,
    pub margins: [f64; 4],
    pub v_align: VerticalAlignment,
    pub text_direction: String,
    pub no_wrap: bool,
    pub fit_text: bool,
    /// `w:hideMark`.
    pub hide_mark: bool,
    /// The diagonal borders, top left to bottom right and top right to bottom left (`w:tl2br`, `w:tr2bl`).
    pub diagonals: Option<Rc<[Option<Border>; 2]>>,
    /// The cell's authored properties (its `w:tcPr` as read, or as table formatting changed it): what re-resolution
    /// merges over the cell's table-style region when its place in the table changes.
    pub direct: super::table_props::CellDirect,
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
    pub id: Option<String>,
    pub headers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub height: Option<(f64, HeightRule)>,
    pub cant_split: bool,
    pub header: bool,
    pub grid_before: usize,
    pub grid_after: usize,
    pub w_before: Option<Width>,
    pub w_after: Option<Width>,
    pub ind: Option<f64>,
    pub jc: Option<Jc>,
    /// The HTML division the row is in (`trPr/w:divId`).
    pub div: Option<Rc<RowDiv>>,
    /// The table borders the row's exceptions (`w:tblPrEx`) give it, in [`Table::borders`] order; `None` for the
    /// table's.
    pub borders: Option<Rc<[Option<Border>; 6]>>,
    /// The row's exception shading (`w:tblPrEx/w:shd`).
    pub shading: Option<u32>,
    /// Its stripe or cross, drawn over [`Row::shading`].
    pub hatch: Option<super::props::Hatch>,
    /// A row read from a later table joined to this one (nothing between them): that table's shading, fill and
    /// hatch, in place of [`Table::shd`]; none for a row whose exceptions' shading has no colour.
    pub table_shading: Option<(Option<u32>, Option<super::props::Hatch>)>,
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
    pub cells: Vec<Cell>,
    /// The controls wrapping the row, outermost first.
    pub controls: Vec<ControlId>,
    /// Where the row was read from (editable reads only): saving keeps a row it still has. A copied row needs none, or
    /// it would be written as the row it was copied from.
    pub origin: Origin,
    /// The row's authored properties and its exceptions to its table's (its `w:trPr` and `w:tblPrEx`; editable reads
    /// only): what a rewritten row's properties are written from.
    pub direct: super::table_props::RowDirect,
    pub exceptions: super::table_props::ExceptionsDirect,
    /// Tracked insertion or deletion of the row, and tracked changes of its properties.
    pub rev: RowRevisions,
}

/// A table's alternative text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableAlt {
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub style_id: Option<String>,
    pub width: Width,
    pub jc: Jc,
    /// The table's own `w:jc`; a layout run after the first row falls back to it.
    pub own_jc: Option<Jc>,
    pub ind: f64,
    /// A `pct` indent, the fraction of the width the table is laid out in; [`Table::ind`] is then 0.
    pub ind_pct: Option<f64>,
    pub ind_set: bool,
    pub borders: [Option<Border>; 6],
    pub fixed: bool,
    pub cell_spacing: f64,
    /// The table's shading (`w:tblPr/w:shd`, its own or its style's), drawn around spaced cells.
    pub shd: Option<super::props::Shd>,
    pub look: Option<Look>,
    pub bands: (i64, i64),
    /// The alternative text's title and description (`w:tblCaption`, `w:tblDescription`).
    pub alt: Option<Rc<TableAlt>>,
    pub floating: Option<TablePos>,
    pub bidi_visual: bool,
    pub grid: Vec<f64>,
    pub rows: Vec<Row>,
    /// Where the table was read from (editable reads only).
    pub origin: Origin,
    /// Identity in layouts ([`next_para_id`]): drawn rows name their table (table handles, border dragging).
    pub id: u64,
    /// The table's authored properties (its `w:tblPr` as read): what re-resolution merges over a new table style's
    /// properties.
    pub direct: super::table_props::TableDirect,
    /// The block-level content controls around it, outermost first.
    pub controls: Vec<ControlId>,
    /// A tracked change of the table's properties, and of its column grid.
    pub change: Option<Rc<TableChange>>,
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

/// A section's settings the Page Setup, Layout and header and footer commands change: size, margins, columns, a
/// different first page, borders, its break, page and line numbering, vertical alignment, text direction and notes.
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
    pub page_borders: Option<Rc<PageBorders>>,
    /// The authored `w:pgBorders`: typed (what isn't modelled, art borders' ids, kept beside), with the element as
    /// written while they are the borders read from it.
    pub page_borders_direct: super::page_borders::PageBordersDirect,
    pub break_kind: SectionBreak,
    pub pg_start: Option<i64>,
    pub pg_fmt: Option<String>,
    pub chapter: Option<(i64, String)>,
    pub line_numbers: Option<LineNumbering>,
    pub v_align: VerticalAlignment,
    pub text_flow: u8,
    pub notes: [NoteProps; 2],
    pub no_endnote: bool,
    /// `w15:footnoteColumns` (0 when absent).
    pub footnote_columns: u32,
    /// `w:formProt`: filling-in-forms protection covers the section (Word writes it only when off).
    pub form_protected: bool,
    /// Its headers and footers (default, first, even); `None`: the section before's.
    pub headers: HfRefs,
    pub footers: HfRefs,
}

/// A section's page size, margins, columns and different first page, borrowed from it or its tracked change: what
/// layout reads per page.
#[derive(Clone, Copy, Debug)]
pub struct PageSetup<'a> {
    pub page_w: f64,
    pub page_h: f64,
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
    pub header: f64,
    pub footer: f64,
    pub gutter: f64,
    pub columns: &'a Columns,
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Columns {
    /// Column widths and the space after each (explicit for unequal columns).
    pub cols: Vec<(f64, f64)>,
    pub equal: bool,
    pub num: usize,
    pub space: f64,
    pub separator: bool,
}

impl Columns {
    /// The most text columns a section has, as in Word.
    pub const MAX: usize = 45;
}

#[derive(Clone, Debug, PartialEq)]
pub struct LineNumbering {
    pub count_by: i64,
    pub start: i64,
    pub distance: f64,
    pub restart: LineNumberingRestart,
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
    pub display: PageBorderDisplay,
    pub surround_header: bool,
    pub surround_footer: bool,
    /// Behind the text and objects they meet (`w:zOrder="back"`); else in front of them.
    pub behind_text: bool,
}

/// Header/footer references per variant: default, first, even.
pub type HfRefs = [Option<HeaderFooterId>; 3];

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
    pub v_align: VerticalAlignment,
    pub page_borders: Option<Rc<PageBorders>>,
    /// The authored `w:pgBorders`, typed, with the element as written while they are the borders read from it.
    pub page_borders_direct: super::page_borders::PageBordersDirect,
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
    pub blocks: Vec<Block>,
    /// A tracked change of the section's properties.
    pub change: Option<Rc<SectionChange>>,
    /// Where it was read from (its page as read; editable reads only): saving rewrites only the page settings that
    /// changed.
    pub origin: Origin,
}

#[derive(Clone, Debug, Default)]
pub struct Settings {
    /// Word's compatibility mode and options.
    pub policy: LayoutPolicy,
    pub default_tab: f64,
    /// `w:characterSpacingControl` is compressPunctuation or compressPunctuationAndJapaneseKana.
    pub compress_punctuation: bool,
    pub even_and_odd: bool,
    pub mirror_margins: bool,
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
    pub list_separator: Option<char>,
    /// `w:bookFoldPrinting` / `w:bookFoldRevPrinting`: `Some(reversed)`.
    pub book_fold: Option<bool>,
    /// `w:strictFirstAndLastChars`: the strict East Asian line-breaking level.
    pub strict_kinsoku: bool,
    pub auto_hyphenation: bool,
    pub hyphenation_zone: f64,
    pub consecutive_hyphen_limit: i64,
    pub do_not_hyphenate_caps: bool,
    pub borders_do_not_surround_header: bool,
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
    pub protection: Option<Protection>,
    /// `w:revisionView` as written, kept for saving.
    pub revision_view_raw: Raw,
    /// `w:revisionView`: the markup the document shows when it opens.
    pub revision_view: super::review::ShowMarkup,
    /// `w:doNotDisplayPageBoundaries`: the white space between pages is hidden when the document opens.
    pub hides_page_boundaries: bool,
    /// `w:displayBackgroundShape`: the page background shows (Word shows none without it).
    pub display_background_shape: bool,
}

/// A theme colour a WordprocessingML colour names (`w:themeColor`), lightened (`w:themeTint`) or darkened
/// (`w:themeShade`) by a byte: the fraction of the colour's luminance kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeColourRef {
    pub slot: crate::drawing::colour::SchemeSlot,
    pub tint: Option<u8>,
    pub shade: Option<u8>,
}

impl Document {
    /// The page background as it shows: its colour and fill, both `None` when Word doesn't display it.
    pub fn shown_background(&self) -> (Option<u32>, Option<&crate::drawing::paint::Fill>) {
        match self.settings.display_background_shape {
            true => (self.page_colour, self.page_fill.as_ref()),
            false => (None, None),
        }
    }
}

impl Settings {
    pub fn compat(&self) -> Compat {
        self.policy.compat()
    }
}

/// A list definition made since reading: its `w:abstractNum` (as written) and the `w:num` that uses it.
#[derive(Clone, Debug, PartialEq)]
pub struct AddedList {
    pub abstract_id: i64,
    pub num_id: i64,
    pub abstract_xml: String,
}

/// Content a document imports in place (`w:altChunk`): Word reads it as content when it opens the document.
#[derive(Clone, Debug)]
pub struct AltChunk {
    /// The empty paragraph standing in its place ([`Paragraph::id`]), whose source is the `w:altChunk` element.
    pub para: u64,
    pub content: AltContent,
}

/// What a `w:altChunk` imports, shared by every one importing the same part.
#[derive(Clone, Debug)]
pub enum AltContent {
    Html(Rc<str>),
    /// Plain text: a paragraph a line.
    Text(Rc<str>),
    /// A document, read: its body comes in, in this document's styles unless `match_src` (`w:matchSrc`).
    Document {
        doc: Rc<Document>,
        match_src: bool,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub sections: Vec<Section>,
    pub theme: Rc<crate::drawing::theme::Theme>,
    pub colour_map: crate::drawing::theme::ColourMap,
    /// The page background's colour (`w:background`), a theme colour's tint or shade applied.
    pub page_colour: Option<u32>,
    /// The page background's fill over its colour (`v:background`): a gradient, pattern or picture.
    pub page_fill: Option<crate::drawing::paint::Fill>,
    /// The theme colour `page_colour` is, when it is one.
    pub page_colour_theme: Option<ThemeColourRef>,
    /// Headers and footers, in id order.
    pub stories: std::collections::BTreeMap<HeaderFooterId, Vec<Block>>,
    pub footnotes: HashMap<i64, Vec<Block>>,
    pub endnotes: HashMap<i64, Vec<Block>>,
    /// Footnote separator / continuation separator stories, when the document defines them.
    pub separators: HashMap<String, Vec<Block>>,
    pub images: Resources,
    /// HTML imported in place (`w:altChunk`), each standing as an empty paragraph until the editor brings it in.
    pub alt_chunks: Vec<AltChunk>,
    /// The core, extended and custom document properties.
    pub properties: super::properties::Properties,
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
    pub settings: Settings,
    pub line_number_run: Option<Run>,
    /// The styles, numbering and theme it resolves with. `Rc`: clipboard fragments carry their source
    /// document's resolver to resolve pasted formatting.
    pub definitions: Option<Rc<super::definitions::Definitions>>,
    /// The font table's description of each family, by lowercase name: what a resolver is told about a missing one.
    pub font_descriptions: HashMap<String, super::font_table::FontDescription>,
    /// Who made the document's revisions and comments.
    pub people: People,
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

impl Tab {
    pub fn is_clear(&self) -> bool {
        self.kind == TabKind::Clear
    }
}
