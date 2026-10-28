//! Document-wide inputs to layout: the settings text layout reads, and the ones page layout adds. Both are
//! compared by equality wherever results are reused; a change sets `Accumulated.full`.

use crate::docx::review::ReviewView;
use crate::docx::{Document, Run};

/// What text layout reads from the document settings. Values are kept as written; each algorithm interprets them
/// (for example `splitPgBreakAndParaMark` only applies in compat 14 or below, COMPAT-3).
#[derive(Clone, Debug, PartialEq)]
pub struct TextEnv {
    /// `w:compatSetting compatibilityMode` (COMPAT-1 to COMPAT-3).
    pub compat: i64,
    /// `w:defaultTabStop`, points (TAB-2).
    pub default_tab: f64,
    pub do_not_expand_shift_return: bool,
    pub split_pg_break_and_para_mark: bool,
    pub allow_space_of_same_style_in_table: bool,
    pub do_not_use_html_paragraph_auto_spacing: bool,
    /// `w:growAutofit` (TABLE-17: tables may grow past the text width).
    pub grow_autofit: bool,
    /// `w:balanceSingleByteDoubleByteWidth` (FONT-14: runs of spaces widen).
    pub balance_spaces: bool,
    /// `w:characterSpacingControl` compressPunctuation (BREAK-8: no break after a hyphen in East Asian runs).
    pub compress_punctuation: bool,
    /// The markup view and author colours: which revisions show, and how.
    pub review: ReviewView,
}

impl TextEnv {
    pub fn new(doc: &Document) -> TextEnv {
        let s = &doc.settings;
        TextEnv {
            compat: s.compat_mode,
            default_tab: s.default_tab,
            do_not_expand_shift_return: s.has("doNotExpandShiftReturn"),
            split_pg_break_and_para_mark: s.has("splitPgBreakAndParaMark"),
            allow_space_of_same_style_in_table: s.has("allowSpaceOfSameStyleInTable"),
            do_not_use_html_paragraph_auto_spacing: s.has("doNotUseHTMLParagraphAutoSpacing"),
            grow_autofit: s.has("growAutofit"),
            balance_spaces: s.has("balanceSingleByteDoubleByteWidth"),
            compress_punctuation: s.compress_punctuation,
            review: doc.review.clone(),
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
        }
    }
}
