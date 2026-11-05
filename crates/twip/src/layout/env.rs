//! Document-wide inputs to layout: the settings text layout reads, and the ones page layout adds. Both are
//! compared by equality wherever results are reused; a change sets `Accumulated.full`.

use crate::docx::Compat;
use crate::docx::review::ReviewView;
use crate::docx::{Document, Run};
use std::rc::Rc;

/// What text layout reads from the document settings. Values are kept as written; each algorithm interprets them
/// (for example `splitPgBreakAndParaMark` only applies in compat 14 or below, COMPAT-3).
#[derive(Clone, Debug, PartialEq)]
pub struct TextEnv {
    /// `w:compatSetting compatibilityMode` (COMPAT-1 to COMPAT-3).
    pub compat: Compat,
    /// `w:defaultTabStop`, points (TAB-2).
    pub default_tab: f64,
    pub do_not_expand_shift_return: bool,
    pub split_pg_break_and_para_mark: bool,
    pub allow_space_of_same_style_in_table: bool,
    pub do_not_use_html_paragraph_auto_spacing: bool,
    /// `w:alignTablesRowByRow` (TABLE-23: compat ≤ 14 aligns each row by its own cells' extent).
    pub align_tables_row_by_row: bool,
    /// `w:doNotVertAlignCellWithSp` (TABLE-30: in compat ≤ 14 a cell holding a float isn't aligned).
    pub do_not_vert_align_cell_with_sp: bool,
    /// `w:doNotBreakWrappedTables` (TABLE-49: in compat ≤ 14 a floating table isn't split).
    pub do_not_break_wrapped_tables: bool,
    /// `w:doNotVertAlignInTxbx` (FLOAT-10: in compat ≤ 14 a text box's text isn't aligned vertically).
    pub do_not_vert_align_in_txbx: bool,
    /// `w:wpJustification` (JUST-7: in compat ≤ 14 a justified line may run past its right edge).
    pub wp_justification: bool,
    /// `w:useWord97LineBreakRules` (FONT-18: in compat ≤ 14 some runs are laid out at a 1000-unit em).
    pub word97_line_breaks: bool,
    /// The default paragraph style: a table row's end mark has it (PARA-4).
    pub default_style: Option<String>,
    /// `w:growAutofit` (TABLE-17: tables may grow past the text width).
    pub grow_autofit: bool,
    /// `w:balanceSingleByteDoubleByteWidth` (FONT-14: spaces next to East Asian text widen).
    pub balance_spaces: bool,
    /// FONT-14: the document default East Asian font (`w:rPrDefault`), empty for the theme's empty slot; `None`
    /// without one.
    pub default_east_asia: Option<String>,
    /// `w:ulTrailSpace` (FONT-15: the spaces that end a line are underlined too).
    pub ul_trail_space: bool,
    /// `w:characterSpacingControl` compressPunctuation (BREAK-8: no break after a hyphen in East Asian runs).
    pub compress_punctuation: bool,
    /// The markup view and author colours: which revisions show, and how.
    pub review: ReviewView,
    /// LINE-16: the section's line grid pitch (layout units) where its lines snap: main text, and cells with
    /// `w:adjustLineHeightInTable`. `None` in headers, footers, notes and text boxes.
    pub line_grid: Option<i32>,
    /// `w:adjustLineHeightInTable` (LINE-16: cell lines snap to the grid).
    pub adjust_line_height_in_table: bool,
    /// The colours shapes resolve against (DRAW-7, DRAW-8): read-only document state.
    pub theme: std::rc::Rc<crate::drawing::theme::Theme>,
    pub colour_map: crate::drawing::theme::ColourMap,
    /// A dark fill behind the story's text (a cell's shading, a text box's fill): automatic text is white over it
    /// (FONT-15). `None` for a light fill or none.
    pub backdrop: Option<u32>,
}

/// Whether automatic text is white over fill `c` (FONT-15): 0.30 R + 0.59 G + 0.11 B under 60.
pub fn is_dark(c: u32) -> bool {
    30 * (c >> 16) + 59 * ((c >> 8) & 0xFF) + 11 * (c & 0xFF) < 6000
}

impl TextEnv {
    pub fn new(doc: &Document) -> TextEnv {
        let s = &doc.settings;
        TextEnv {
            compat: s.compat(),
            default_tab: s.default_tab,
            do_not_expand_shift_return: s.has("doNotExpandShiftReturn"),
            split_pg_break_and_para_mark: s.has("splitPgBreakAndParaMark"),
            allow_space_of_same_style_in_table: s.has("allowSpaceOfSameStyleInTable"),
            do_not_use_html_paragraph_auto_spacing: s.has("doNotUseHTMLParagraphAutoSpacing"),
            align_tables_row_by_row: s.has("alignTablesRowByRow"),
            do_not_vert_align_cell_with_sp: s.has("doNotVertAlignCellWithSp"),
            do_not_break_wrapped_tables: s.has("doNotBreakWrappedTables"),
            do_not_vert_align_in_txbx: s.has("doNotVertAlignInTxbx"),
            wp_justification: s.has("wpJustification"),
            word97_line_breaks: s.has("useWord97LineBreakRules"),
            default_style: doc.resolver.as_ref().and_then(|r| r.styles.default_paragraph.clone()),
            grow_autofit: s.has("growAutofit"),
            balance_spaces: s.has("balanceSingleByteDoubleByteWidth"),
            ul_trail_space: s.has("ulTrailSpace"),
            compress_punctuation: s.compress_punctuation,
            review: doc.review.clone(),
            line_grid: None,
            default_east_asia: doc.resolver.as_ref().and_then(|r| match &r.styles.doc_rpr.east_asia {
                Some(crate::docx::props::FontRef::Theme(t)) if r.theme.ea_unset(t) => Some(String::new()),
                f => r.font(f),
            }),
            adjust_line_height_in_table: s.has("adjustLineHeightInTable"),
            theme: doc.theme.clone(),
            colour_map: doc.colour_map,
            backdrop: None,
        }
    }
}

/// What page layout reads besides [`TextEnv`].
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutEnv {
    pub text: TextEnv,
    /// `w:evenAndOddHeaders` (HDR-7).
    pub even_and_odd: bool,
    /// `w:mirrorMargins` and `w:gutterAtTop` (HDR-13).
    pub mirror_margins: bool,
    pub gutter_at_top: bool,
    /// The resolved `LineNumber` character style (LNUM-1).
    pub line_number_run: Option<Run>,
    /// Linked text boxes: each chain's story and its boxes in order.
    pub chains: Rc<[Chain]>,
}

/// A chain of linked text boxes (`wps:txbx id` in the first, `wps:linkedTxbx id seq` in the others): the first box's
/// story, laid out box after box; each box's float id and text frame size (width, height), first box first.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub story: crate::docx::TextBox,
    pub boxes: Vec<(u64, [f64; 2])>,
}

impl LayoutEnv {
    /// The chain float `f` is a box of, and its place in it.
    pub fn chain_of(&self, f: u64) -> Option<(&Chain, usize)> {
        self.chains.iter().find_map(|c| c.boxes.iter().position(|b| b.0 == f).map(|k| (c, k)))
    }
}

/// The document's chains of linked text boxes (those whose first box is in the document and that have another box).
fn chains(doc: &Document) -> Rc<[Chain]> {
    use crate::drawing::ShapeText;
    use std::collections::BTreeMap;
    // By chain id: the first box's story and frame, and the later boxes by their seq.
    let mut heads: BTreeMap<u32, (crate::docx::TextBox, (u64, [f64; 2]))> = BTreeMap::new();
    let mut later: BTreeMap<u32, Vec<(u32, (u64, [f64; 2]))>> = BTreeMap::new();
    fn walk(blocks: &[crate::docx::Block], out: &mut dyn FnMut(&crate::docx::Float)) {
        for b in blocks {
            match b {
                crate::docx::Block::Para(p) => p.inlines.iter().for_each(|i| {
                    if let crate::docx::Inline::Float(f, _) = i {
                        out(f);
                    }
                }),
                crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut each = |f: &crate::docx::Float| {
        let Some(sp) = f.shape.as_deref().and_then(|s| s.sp()) else { return };
        match (&sp.text, sp.chain, &f.text_box) {
            (ShapeText::Story, Some(c), Some(tb)) => {
                heads.insert(c, (tb.clone(), (f.id, super::story::text_frame_size(f))));
            }
            (ShapeText::Linked { chain, seq }, ..) => later.entry(*chain).or_default().push((*seq, (f.id, super::story::text_frame_size(f)))),
            _ => {}
        }
    };
    doc.sections.iter().for_each(|s| walk(&s.blocks, &mut each));
    doc.stories.values().for_each(|b| walk(b, &mut each));
    heads
        .into_iter()
        .filter_map(|(id, (story, first))| {
            let mut rest = later.remove(&id)?;
            rest.sort_by_key(|(seq, _)| *seq);
            Some(Chain { story, boxes: std::iter::once(first).chain(rest.into_iter().map(|(_, b)| b)).collect() })
        })
        .collect()
}

impl LayoutEnv {
    pub fn new(doc: &Document) -> LayoutEnv {
        let s = &doc.settings;
        LayoutEnv {
            text: TextEnv::new(doc),
            even_and_odd: s.even_and_odd,
            mirror_margins: s.mirror_margins,
            gutter_at_top: s.gutter_at_top,
            line_number_run: doc.line_number_run.clone(),
            chains: chains(doc),
        }
    }
}
