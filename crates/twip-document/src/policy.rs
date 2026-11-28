//! Layout policy

/// Which generation of Word's layout a document asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Compat {
    /// Word 2010 and before: compatibility mode 14 or lower.
    #[default]
    Legacy,
    /// Word 2013 and after: compatibility mode 15.
    Generic,
}

/// The layout behaviours a document asks for: Word's compatibility mode and the compatibility options layout and
/// editing read, each a field. The reader fills it in from the settings part.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutPolicy {
    /// Word's compatibility mode (11 to 15).
    pub mode: i64,
    /// `adjustLineHeightInTable`.
    pub adjust_line_height_in_table: bool,
    /// `alignTablesRowByRow`.
    pub align_tables_row_by_row: bool,
    /// `allowHyphenationAtTrackBottom`.
    pub allow_hyphenation_at_track_bottom: bool,
    /// `allowSpaceOfSameStyleInTable`.
    pub allow_space_of_same_style_in_table: bool,
    /// `allowTextAfterFloatingTableBreak`.
    pub allow_text_after_floating_table_break: bool,
    /// `applyBreakingRules`.
    pub apply_breaking_rules: bool,
    /// `balanceSingleByteDoubleByteWidth`.
    pub balance_single_byte_double_byte_width: bool,
    /// `doNotAutofitConstrainedTables`.
    pub do_not_autofit_constrained_tables: bool,
    /// `doNotBreakWrappedTables`.
    pub do_not_break_wrapped_tables: bool,
    /// `doNotExpandShiftReturn`.
    pub do_not_expand_shift_return: bool,
    /// `doNotSuppressIndentation`.
    pub do_not_suppress_indentation: bool,
    /// `doNotUseHTMLParagraphAutoSpacing`.
    pub do_not_use_html_paragraph_auto_spacing: bool,
    /// `doNotUseIndentAsNumberingTabStop`.
    pub do_not_use_indent_as_numbering_tab_stop: bool,
    /// `doNotVertAlignCellWithSp`.
    pub do_not_vert_align_cell_with_sp: bool,
    /// `enableOpenTypeFeatures`.
    pub enable_open_type_features: bool,
    /// `footnoteLayoutLikeWW8`.
    pub footnote_layout_like_ww8: bool,
    /// `growAutofit`.
    pub grow_autofit: bool,
    /// `layoutRawTableWidth`.
    pub layout_raw_table_width: bool,
    /// `mwSmallCaps`.
    pub mw_small_caps: bool,
    /// `noColumnBalance`.
    pub no_column_balance: bool,
    /// `noExtraLineSpacing`.
    pub no_extra_line_spacing: bool,
    /// `noLeading`.
    pub no_leading: bool,
    /// `noSpaceRaiseLower`.
    pub no_space_raise_lower: bool,
    /// `noTabHangInd`.
    pub no_tab_hang_ind: bool,
    /// `overrideTableStyleFontSizeAndJustification`.
    pub override_table_style_font_size_and_justification: bool,
    /// `showBreaksInFrames`.
    pub show_breaks_in_frames: bool,
    /// `spacingInWholePoints`.
    pub spacing_in_whole_points: bool,
    /// `splitPgBreakAndParaMark`.
    pub split_pg_break_and_para_mark: bool,
    /// `suppressBottomSpacing`.
    pub suppress_bottom_spacing: bool,
    /// `suppressSpBfAfterPgBrk`.
    pub suppress_sp_bf_after_pg_brk: bool,
    /// `suppressSpacingAtTopOfPage`.
    pub suppress_spacing_at_top_of_page: bool,
    /// `suppressTopSpacing`.
    pub suppress_top_spacing: bool,
    /// `suppressTopSpacingWP`.
    pub suppress_top_spacing_wp: bool,
    /// `ulTrailSpace`.
    pub ul_trail_space: bool,
    /// `useSingleBorderforContiguousCells`.
    pub use_single_border_for_contiguous_cells: bool,
    /// `useWord2002TableStyleRules`.
    pub use_word2002_table_style_rules: bool,
    /// `useWord97LineBreakRules`.
    pub use_word97_line_break_rules: bool,
    /// `wpJustification`.
    pub wp_justification: bool,
    /// `useWord2013TrackBottomHyphenation` set off.
    pub word2013_track_bottom_hyphenation_off: bool,
    /// An option is on that no field above names.
    pub other: bool,
}

impl LayoutPolicy {
    /// The layout generation its mode gives.
    pub fn compat(&self) -> Compat {
        if self.mode >= 15 { Compat::Generic } else { Compat::Legacy }
    }

    /// Whether any option is on, whatever the mode.
    pub fn any_option(&self) -> bool {
        *self != LayoutPolicy { mode: self.mode, ..LayoutPolicy::default() }
    }
}
