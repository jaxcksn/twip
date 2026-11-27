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
    /// `layoutRawTableWidth`: a table row beside a float reports its cells' extent, without the indent and the last
    /// cell's right margin.
    pub raw_table_width: bool,
    /// Compat ≤ 14: the story is a text box's. A table at its top level has a twip less room, and its pct width and
    /// pct indent are shares of the box's text width and 15 twips more.
    pub box_story: bool,
    /// `doNotAutofitConstrainedTables`: an autofit table beside a float keeps the column width it was fitted to.
    pub no_constrained_autofit: bool,
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
    /// Compatibility mode 14 and later: more characters break after ([`super::space::MetricRules::dash_breaks`]).
    pub dash_breaks: bool,
    /// `strictFirstAndLastChars`.
    pub strict_kinsoku: bool,
    pub default_style: Option<std::rc::Rc<str>>,
    pub grow_autofit: bool,
    pub balance_spaces: bool,
    pub default_east_asia: Option<std::rc::Rc<str>>,
    pub ul_trail_space: bool,
    pub compress_punctuation: bool,
    pub review: ReviewView,
    pub line_grid: Option<i32>,
    pub adjust_line_height_in_table: bool,
    pub theme: std::rc::Rc<crate::drawing::theme::Theme>,
    pub colour_map: crate::drawing::theme::ColourMap,
    pub backdrop: Option<u32>,
    /// The document's page colour (`w:background`): what emboss and engrave tone their light copies from where nothing
    /// else is behind the text.
    pub page_colour: Option<u32>,
    /// The vertical flow the text is laid out in.
    pub vertical: Option<super::vertical::Vertical>,
    /// Automatic hyphenation.
    pub hyphenation: Option<HyphenEnv>,
    /// Compat 15, the last line of a page or column without `allowHyphenationAtTrackBottom`: re-broken without
    /// automatic hyphenation (`useWord2013TrackBottomHyphenation` off), else moved to the next one when it ends at a
    /// hyphenation point.
    pub bottom_unhyphenated: bool,
    pub bottom_hyphen_moves: bool,
    /// Compat 15, `allowTextAfterFloatingTableBreak`: the text after a floating table split across pages goes on
    /// beside its first part.
    pub text_after_table_break: bool,
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
    /// The document's text environment, hyphenated with the host's `hyphenation` patterns when it asks for it.
    pub fn new(doc: &Document, hyphenation: Option<&super::hyphenate::Hyphenation>) -> TextEnv {
        let s = &doc.settings;
        let bottom = s.compat() == Compat::Generic && !s.policy.allow_hyphenation_at_track_bottom;
        TextEnv {
            compat: s.compat(),
            default_tab: s.default_tab,
            do_not_expand_shift_return: s.policy.do_not_expand_shift_return,
            split_pg_break_and_para_mark: s.policy.split_pg_break_and_para_mark,
            allow_space_of_same_style_in_table: s.policy.allow_space_of_same_style_in_table,
            do_not_use_html_paragraph_auto_spacing: s.policy.do_not_use_html_paragraph_auto_spacing,
            align_tables_row_by_row: s.policy.align_tables_row_by_row,
            do_not_vert_align_cell_with_sp: s.policy.do_not_vert_align_cell_with_sp,
            do_not_break_wrapped_tables: s.policy.do_not_break_wrapped_tables,
            raw_table_width: s.policy.layout_raw_table_width,
            box_story: false,
            no_constrained_autofit: s.policy.do_not_autofit_constrained_tables,
            wp_justification: s.policy.wp_justification,
            word97_line_breaks: s.policy.use_word97_line_break_rules,
            no_leading: s.compat() == Compat::Legacy && s.policy.no_leading,
            no_raise_lower: s.compat() == Compat::Legacy && s.policy.no_space_raise_lower,
            whole_point_spacing: s.compat() == Compat::Legacy && s.policy.spacing_in_whole_points,
            mac_small_caps: s.compat() == Compat::Legacy && s.policy.mw_small_caps,
            wp_line_heights: s.compat() == Compat::Legacy && s.policy.suppress_top_spacing_wp,
            exact_on_top: s.policy.no_extra_line_spacing,
            exact_extra_after: s.policy.suppress_bottom_spacing,
            drop_cut_indents: s.compat() == Compat::Legacy && s.policy.do_not_suppress_indentation,
            no_tab_hang_ind: s.compat() == Compat::Legacy && s.policy.no_tab_hang_ind,
            no_number_hang_stop: s.compat() == Compat::Legacy && s.policy.do_not_use_indent_as_numbering_tab_stop,
            show_breaks_in_frames: s.compat() == Compat::Legacy && s.policy.show_breaks_in_frames,
            suppress_top_spacing: s.policy.suppress_top_spacing,
            suppress_spacing_at_top: s.compat() == Compat::Legacy && s.policy.suppress_spacing_at_top_of_page,
            suppress_before_after_page_break: s.compat() == Compat::Legacy && s.policy.suppress_sp_bf_after_pg_brk,
            single_cell_borders: s.compat() == Compat::Legacy && s.policy.use_single_border_for_contiguous_cells,
            breaking_rules: s.policy.mode >= 14 || s.policy.apply_breaking_rules,
            dash_breaks: s.policy.mode >= 14,
            strict_kinsoku: s.strict_kinsoku,
            default_style: doc.resolver.as_ref().and_then(|r| r.cascade().styles.default_paragraph.as_deref().map(Into::into)),
            grow_autofit: s.compat() == Compat::Legacy && s.policy.grow_autofit,
            balance_spaces: s.policy.balance_single_byte_double_byte_width,
            ul_trail_space: s.policy.ul_trail_space,
            compress_punctuation: s.compress_punctuation,
            review: doc.review.clone(),
            line_grid: None,
            default_east_asia: doc.resolver.as_ref().and_then(|r| {
                let c = r.cascade();
                crate::format::resolve::default_east_asian(&c.theme, &c.doc_char().fonts.east_asian).map(Into::into)
            }),
            adjust_line_height_in_table: s.policy.adjust_line_height_in_table,
            theme: doc.theme.clone(),
            colour_map: doc.colour_map,
            backdrop: None,
            page_colour: doc.shown_background().0,
            vertical: None,
            hyphenation: hyphenation.filter(|_| s.auto_hyphenation).map(|patterns| HyphenEnv {
                patterns: patterns.clone(),
                zone: s.hyphenation_zone,
                limit: s.consecutive_hyphen_limit,
                caps: s.do_not_hyphenate_caps,
            }),
            bottom_unhyphenated: bottom && s.policy.word2013_track_bottom_hyphenation_off,
            bottom_hyphen_moves: bottom && !s.policy.word2013_track_bottom_hyphenation_off,
            text_after_table_break: s.compat() == Compat::Generic && s.policy.allow_text_after_floating_table_break,
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
    /// The default paragraph style's properties: a header's, footer's, text box's or notes story's own final mark.
    pub normal: Option<Rc<crate::docx::ParaStyle>>,
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

/// Whether `blocks` hold a box of a text box chain: one whose story goes on in others, or one going on with it.
pub fn chained(blocks: &[crate::docx::Block]) -> bool {
    use crate::drawing::ShapeText;
    let mut found = false;
    walk(blocks, &mut |f| {
        found |= f
            .shape
            .as_deref()
            .and_then(|s| s.sp())
            .is_some_and(|sp| matches!((&sp.text, sp.chain), (ShapeText::Story, Some(_)) | (ShapeText::Linked { .. }, _)));
    });
    found
}

fn chains(doc: &Document) -> Rc<[Chain]> {
    use crate::drawing::ShapeText;
    use std::collections::BTreeMap;
    let mut heads: BTreeMap<u32, (crate::docx::TextBox, (u64, [f64; 2]))> = BTreeMap::new();
    let mut later: BTreeMap<u32, Vec<(u32, (u64, [f64; 2]))>> = BTreeMap::new();
    let mut each = |f: &crate::docx::Float| {
        let Some(sp) = f.shape.as_deref().and_then(|s| s.sp()) else { return };
        match (&sp.text, sp.chain, &f.text_box) {
            (ShapeText::Story, Some(c), Some(tb)) => {
                heads.insert(c, (tb.clone(), (f.id, super::story::text_frame_size(f, &doc.theme))));
            }
            (ShapeText::Linked { chain, seq }, ..) => {
                later.entry(*chain).or_default().push((*seq, (f.id, super::story::text_frame_size(f, &doc.theme))))
            }
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

#[cfg(test)]
thread_local! {
    // Test hooks
    pub(crate) static BUILT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

impl LayoutEnv {
    /// The document's environment, with the host's `hyphenation` patterns.
    pub fn new(doc: &Document, hyphenation: Option<&super::hyphenate::Hyphenation>) -> LayoutEnv {
        LayoutEnv::with_chains(doc, None, hyphenation)
    }

    /// [`LayoutEnv::new`] with the document's text box chains when they are known.
    pub fn with_chains(doc: &Document, chains: Option<Rc<[Chain]>>, hyphenation: Option<&super::hyphenate::Hyphenation>) -> LayoutEnv {
        #[cfg(test)]
        BUILT.set(BUILT.get() + 1);
        let s = &doc.settings;
        LayoutEnv {
            text: TextEnv::new(doc, hyphenation),
            even_and_odd: s.even_and_odd,
            mirror_margins: s.mirror_margins,
            gutter_at_top: s.gutter_at_top,
            book_fold: s.book_fold,
            line_number_run: doc.line_number_run.clone(),
            chains: chains.unwrap_or_else(|| self::chains(doc)),
            notes_like_ww8: s.compat() == Compat::Legacy && s.policy.footnote_layout_like_ww8,
            normal: doc.resolver.as_ref().map(|r| Rc::new(crate::docx::para_xml::resolve(r, &Default::default(), None, None).style)),
        }
    }
}
