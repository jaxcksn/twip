//! Paragraph spacing (PARA-1 to PARA-5) and how many of a paragraph's lines a column takes (PAGE-3 widow/orphan
//! control, PAGE-6 keepLines), separate from the column fill so they can be tested alone.

use crate::docx::ParaStyle;

use super::env::TextEnv;

/// PARA-5: automatic spacing is 14pt, 5pt with doNotUseHTMLParagraphAutoSpacing.
fn auto_space(env: &TextEnv) -> f64 {
    if env.do_not_use_html_paragraph_auto_spacing { 5.0 } else { 14.0 }
}

pub fn space_before(env: &TextEnv, s: &ParaStyle) -> f64 {
    if s.before_auto { auto_space(env) } else { s.space_before }
}

pub fn space_after(env: &TextEnv, s: &ParaStyle) -> f64 {
    if s.after_auto { auto_space(env) } else { s.space_after }
}

/// PARA-1 without contextual spacing.
pub fn gap_plain(env: &TextEnv, prev_after: f64, before: f64) -> f64 {
    if env.do_not_use_html_paragraph_auto_spacing { prev_after + before } else { prev_after.max(before) }
}

/// The space between two paragraphs (PARA-1, PARA-4, PARA-5).
pub fn gap(env: &TextEnv, prev: &ParaStyle, prev_after: f64, next: &ParaStyle, before: f64) -> f64 {
    // PARA-5: no automatic spacing between two paragraphs of the same list (abstractNum), except with
    // doNotUseHTMLParagraphAutoSpacing.
    let same_list = prev.list.is_some() && prev.list == next.list && !env.do_not_use_html_paragraph_auto_spacing;
    let prev_after = if same_list && prev.after_auto { 0.0 } else { prev_after };
    let before = if same_list && next.before_auto { 0.0 } else { before };
    // PARA-4: the gap is the space-after plus what the space-before adds to it; between paragraphs of one style, each
    // contextual one drops its own part.
    let same = prev.style_id == next.style_id;
    let extra = gap_plain(env, prev_after, before) - prev_after;
    (if prev.contextual && same { 0.0 } else { prev_after }) + (if next.contextual && same { 0.0 } else { extra })
}

/// PARA-4 at a cell or table edge, where nothing collapses: whether paragraph `p` drops its space on the side facing
/// `other`, its neighbour in document order (a table row's end mark has the default paragraph style).
pub fn drops_at_edge(p: &ParaStyle, other: Option<&Option<String>>) -> bool {
    p.contextual && other.is_some_and(|o| *o == p.style_id)
}

/// Whether contextual spacing applies here (PARA-4).
pub fn contextual_in(env: &TextEnv, in_cell: bool) -> bool {
    !(in_cell && env.compat <= 14 && env.allow_space_of_same_style_in_table)
}

/// How many of a paragraph's `n` lines to place when `fit` of them fit in the column (`hard`: one of the fitting
/// lines ends in a page or column break): PAGE-3 widow/orphan control, PAGE-6 keepLines. 0 = none here (the caller
/// decides whether an empty column forces progress, PAGE-5).
pub fn lines_to_take(n: usize, fit: usize, hard: bool, first_part: bool, keep_lines: bool, widow_control: bool) -> usize {
    let min_lines = if widow_control { 2 } else { 1 };
    let mut take = fit;
    if !hard && fit < n {
        let rest = n - fit;
        if first_part && (keep_lines || fit < min_lines) {
            take = 0;
        } else if rest < min_lines {
            let k = fit as i64 - (min_lines - rest) as i64;
            take = if k < if first_part { min_lines as i64 } else { 1 } { 0 } else { k as usize };
        }
    }
    take
}

#[cfg(test)]
mod tests {
    use super::lines_to_take;

    #[test]
    fn everything_that_fits_is_taken() {
        assert_eq!(lines_to_take(5, 5, false, true, false, true), 5);
    }

    #[test]
    fn page_3_orphan_control_needs_two_lines_at_the_bottom() {
        // One line of a five-line paragraph fits: with widow control it moves on whole.
        assert_eq!(lines_to_take(5, 1, false, true, false, true), 0);
        // Without widow control the one line stays.
        assert_eq!(lines_to_take(5, 1, false, true, false, false), 1);
    }

    #[test]
    fn page_3_widow_control_leaves_two_lines_for_the_next_page() {
        // Four of five fit: one would be left alone, so three are taken.
        assert_eq!(lines_to_take(5, 4, false, true, false, true), 3);
        // Two of three fit: taking one would leave an orphan, so none.
        assert_eq!(lines_to_take(3, 2, false, true, false, true), 0);
        // A continuation may keep a single line here.
        assert_eq!(lines_to_take(3, 2, false, false, false, true), 1);
    }

    #[test]
    fn page_6_keep_lines_moves_a_first_part_whole() {
        assert_eq!(lines_to_take(5, 4, false, true, true, false), 0);
        // A continuation of a keepLines paragraph (it didn't fit a whole column) splits normally.
        assert_eq!(lines_to_take(5, 4, false, false, true, false), 4);
    }

    #[test]
    fn para_4_each_contextual_paragraph_drops_its_own_part_of_the_gap() {
        use super::gap;
        use crate::docx::Block;
        use crate::layout::env::TextEnv;
        let Some(doc) = crate::layout::tests::plain_doc(&[String::new()]) else { return };
        let Block::Para(p) = &doc.sections[0].blocks[0] else { unreachable!() };
        let env = TextEnv::new(&doc);
        let style = |contextual| crate::docx::ParaStyle { contextual, ..(*p.style).clone() };
        let (plain, ctx) = (style(false), style(true));
        // The space-after (10), and what the space-before (6) adds to it (nothing).
        assert_eq!(gap(&env, &plain, 10.0, &plain, 6.0), 10.0);
        assert_eq!(gap(&env, &ctx, 10.0, &plain, 6.0), 0.0);
        assert_eq!(gap(&env, &ctx, 0.0, &plain, 6.0), 6.0);
        assert_eq!(gap(&env, &plain, 8.0, &ctx, 0.0), 8.0);
        assert_eq!(gap(&env, &ctx, 6.0, &ctx, 12.0), 0.0);
    }

    #[test]
    fn para_5_no_automatic_spacing_between_items_of_one_list() {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(crate::docx::Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let env = super::TextEnv::new(&doc);
        let item = crate::docx::ParaStyle { before_auto: true, after_auto: true, num: Some((1, 0)), list: Some(0), ..(*p.style).clone() };
        assert_eq!(super::gap(&env, &item, 14.0, &item, 14.0), 0.0);
        // Another numId (or level) on the same abstractNum is the same list.
        let other_num = crate::docx::ParaStyle { num: Some((2, 1)), ..item.clone() };
        assert_eq!(super::gap(&env, &item, 14.0, &other_num, 14.0), 0.0);
        // An explicit side stays.
        let explicit = crate::docx::ParaStyle { before_auto: false, space_before: 12.0, ..item.clone() };
        assert_eq!(super::gap(&env, &item, 14.0, &explicit, 12.0), 12.0);
        // A paragraph outside the list (or in another abstract list) keeps its automatic 14pt.
        let plain = crate::docx::ParaStyle { num: None, list: None, ..item.clone() };
        assert_eq!(super::gap(&env, &item, 14.0, &plain, 14.0), 14.0);
        let other_list = crate::docx::ParaStyle { list: Some(1), ..item.clone() };
        assert_eq!(super::gap(&env, &item, 14.0, &other_list, 14.0), 14.0);
    }

    #[test]
    fn para_5_without_html_spacing_automatic_spacing_is_5pt_and_adds_in_lists_too() {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(crate::docx::Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let env = super::TextEnv { do_not_use_html_paragraph_auto_spacing: true, ..super::TextEnv::new(&doc) };
        let item = crate::docx::ParaStyle { before_auto: true, after_auto: true, list: Some(0), ..(*p.style).clone() };
        let (after, before) = (super::space_after(&env, &item), super::space_before(&env, &item));
        assert_eq!((after, before), (5.0, 5.0));
        assert_eq!(super::gap(&env, &item, after, &item, before), 10.0);
    }

    #[test]
    fn a_hard_break_takes_what_fits() {
        assert_eq!(lines_to_take(5, 2, true, true, true, true), 2);
    }
}
