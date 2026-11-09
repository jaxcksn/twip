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
    pub line_number_run: Option<Run>,
    pub chains: Rc<[Chain]>,
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
            line_number_run: doc.line_number_run.clone(),
            chains: chains(doc),
        }
    }
}
