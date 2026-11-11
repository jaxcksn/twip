//! Layout environment

use crate::docx::Compat;
use crate::docx::review::ReviewView;
use crate::docx::{Document, Run};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub struct TextEnv {
    pub compat: Compat,
    pub default_tab: f64,
    pub do_not_expand_shift_return: bool,
    pub split_pg_break_and_para_mark: bool,
    pub allow_space_of_same_style_in_table: bool,
    pub do_not_use_html_paragraph_auto_spacing: bool,
    pub align_tables_row_by_row: bool,
    pub do_not_vert_align_cell_with_sp: bool,
    pub do_not_break_wrapped_tables: bool,
    pub do_not_vert_align_in_txbx: bool,
    pub wp_justification: bool,
    pub word97_line_breaks: bool,
    /// Font metric options, compat ≤ 14: `noLeading` (no external leading), `noSpaceRaiseLower` (raised and
    /// lowered text doesn't grow the line), `spacingInWholePoints` (character spacing truncated to whole points),
    /// `mwSmallCaps` (the Mac small-caps sizes).
    pub no_leading: bool,
    pub no_raise_lower: bool,
    pub whole_point_spacing: bool,
    pub mac_small_caps: bool,
    /// `suppressTopSpacingWP`, compat ≤ 14: line heights of Latin fonts from the font size.
    pub wp_line_heights: bool,
    /// Exact line heights: `noExtraLineSpacing` puts the baseline at the line's ascent; `suppressBottomSpacing` makes
    /// the part of the room under the baseline beyond the line's descent space below the text.
    pub exact_on_top: bool,
    pub exact_extra_after: bool,
    /// `doNotSuppressIndentation`, compat ≤ 14: beside a float a line drops the indents of its cut sides.
    pub drop_cut_indents: bool,
    /// The hanging indent is no tab stop: `noTabHangInd` (any tab) and `doNotUseIndentAsNumberingTabStop` (a
    /// list number's tab), compat ≤ 14.
    pub no_tab_hang_ind: bool,
    pub no_number_hang_stop: bool,
    /// `showBreaksInFrames`, compat ≤ 14: page and column breaks in a frame break its lines.
    pub show_breaks_in_frames: bool,
    /// `suppressTopSpacing`.
    pub suppress_top_spacing: bool,
    /// `suppressSpacingAtTopOfPage`, compat ≤ 14.
    pub suppress_spacing_at_top: bool,
    /// `suppressSpBfAfterPgBrk`, compat ≤ 14: the text after a paragraph's leading page break loses its space before.
    pub suppress_before_after_page_break: bool,
    /// `useSingleBorderforContiguousCells`, compat ≤ 14: a shared cell edge takes the right or lower cell's border.
    pub single_cell_borders: bool,
    /// East Asian line-breaking rules: `applyBreakingRules`, or compatibility mode 14 and later.
    pub breaking_rules: bool,
    /// `strictFirstAndLastChars`.
    pub strict_kinsoku: bool,
    pub default_style: Option<String>,
    pub grow_autofit: bool,
    pub balance_spaces: bool,
    pub default_east_asia: Option<String>,
    pub ul_trail_space: bool,
    pub compress_punctuation: bool,
    pub review: ReviewView,
    pub line_grid: Option<i32>,
    pub adjust_line_height_in_table: bool,
    pub theme: std::rc::Rc<crate::drawing::theme::Theme>,
    pub colour_map: crate::drawing::theme::ColourMap,
    pub backdrop: Option<u32>,
    /// The vertical flow the text is laid out in.
    pub vertical: Option<super::vertical::Vertical>,
    /// Automatic hyphenation.
    pub hyphenation: Option<HyphenEnv>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HyphenEnv {
    pub patterns: super::hyphenate::Hyphenation,
    pub zone: f64,
    pub limit: i64,
    pub caps: bool,
}

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
            no_leading: s.compat() == Compat::Legacy && s.has("noLeading"),
            no_raise_lower: s.compat() == Compat::Legacy && s.has("noSpaceRaiseLower"),
            whole_point_spacing: s.compat() == Compat::Legacy && s.has("spacingInWholePoints"),
            mac_small_caps: s.compat() == Compat::Legacy && s.has("mwSmallCaps"),
            wp_line_heights: s.compat() == Compat::Legacy && s.has("suppressTopSpacingWP"),
            exact_on_top: s.has("noExtraLineSpacing"),
            exact_extra_after: s.has("suppressBottomSpacing"),
            drop_cut_indents: s.compat() == Compat::Legacy && s.has("doNotSuppressIndentation"),
            no_tab_hang_ind: s.compat() == Compat::Legacy && s.has("noTabHangInd"),
            no_number_hang_stop: s.compat() == Compat::Legacy && s.has("doNotUseIndentAsNumberingTabStop"),
            show_breaks_in_frames: s.compat() == Compat::Legacy && s.has("showBreaksInFrames"),
            suppress_top_spacing: s.has("suppressTopSpacing"),
            suppress_spacing_at_top: s.compat() == Compat::Legacy && s.has("suppressSpacingAtTopOfPage"),
            suppress_before_after_page_break: s.compat() == Compat::Legacy && s.has("suppressSpBfAfterPgBrk"),
            single_cell_borders: s.compat() == Compat::Legacy && s.has("useSingleBorderforContiguousCells"),
            breaking_rules: s.compat_mode >= 14 || s.has("applyBreakingRules"),
            strict_kinsoku: s.strict_kinsoku,
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
            vertical: None,
            hyphenation: doc.hyphenation.clone().filter(|_| s.auto_hyphenation).map(|patterns| HyphenEnv {
                patterns,
                zone: s.hyphenation_zone,
                limit: s.consecutive_hyphen_limit,
                caps: s.do_not_hyphenate_caps,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutEnv {
    pub text: TextEnv,
    pub even_and_odd: bool,
    pub mirror_margins: bool,
    pub gutter_at_top: bool,
    pub book_fold: Option<bool>,
    pub line_number_run: Option<Run>,
    pub chains: Rc<[Chain]>,
    /// `footnoteLayoutLikeWW8`, compat ≤ 14: a continuous section goes on above the page-bottom footnotes of an
    /// earlier section on the page, up to its first line with a footnote reference.
    pub notes_like_ww8: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub story: crate::docx::TextBox,
    pub boxes: Vec<(u64, [f64; 2])>,
}

impl LayoutEnv {
    pub fn chain_of(&self, f: u64) -> Option<(&Chain, usize)> {
        self.chains.iter().find_map(|c| c.boxes.iter().position(|b| b.0 == f).map(|k| (c, k)))
    }
}

fn chains(doc: &Document) -> Rc<[Chain]> {
    use crate::drawing::ShapeText;
    use std::collections::BTreeMap;
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
            book_fold: s.book_fold,
            line_number_run: doc.line_number_run.clone(),
            chains: chains(doc),
            notes_like_ww8: s.compat() == Compat::Legacy && s.has("footnoteLayoutLikeWW8"),
        }
    }
}
