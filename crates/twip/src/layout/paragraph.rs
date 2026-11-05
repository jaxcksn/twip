//! Paragraph spacing and splitting

use crate::docx::Compat;
use crate::docx::ParaStyle;

use super::env::TextEnv;

fn auto(env: &TextEnv, on: bool) -> bool {
    on && !env.do_not_use_html_paragraph_auto_spacing
}

pub fn space_before(env: &TextEnv, s: &ParaStyle) -> f64 {
    if auto(env, s.before_auto) { 14.0 } else { s.space_before }
}

pub fn space_after(env: &TextEnv, s: &ParaStyle) -> f64 {
    if auto(env, s.after_auto) { 14.0 } else { s.space_after }
}

pub fn gap_plain(env: &TextEnv, prev_after: f64, before: f64) -> f64 {
    if env.do_not_use_html_paragraph_auto_spacing { prev_after + before } else { prev_after.max(before) }
}

pub fn gap(env: &TextEnv, prev: &ParaStyle, prev_after: f64, next: &ParaStyle, before: f64) -> f64 {
    let same_list = prev.list.is_some() && prev.list == next.list && !env.do_not_use_html_paragraph_auto_spacing;
    let prev_after = if same_list && prev.after_auto { 0.0 } else { prev_after };
    let before = if same_list && next.before_auto { 0.0 } else { before };
    let same = prev.style_id == next.style_id;
    let extra = gap_plain(env, prev_after, before) - prev_after;
    (if prev.contextual && same { 0.0 } else { prev_after }) + (if next.contextual && same { 0.0 } else { extra })
}

pub fn drops_at_edge(p: &ParaStyle, other: Option<&Option<String>>) -> bool {
    p.contextual && other.is_some_and(|o| *o == p.style_id)
}

pub fn contextual_in(env: &TextEnv, in_cell: bool) -> bool {
    !(in_cell && env.compat == Compat::Legacy && env.allow_space_of_same_style_in_table)
}

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

pub fn vanished(p: &crate::docx::Paragraph) -> bool {
    use crate::docx::Inline;
    p.mark.hidden
        && p.list.is_none()
        && p.inlines.iter().all(|i| match i {
            Inline::Text(t, r) => t.is_empty() || r.hidden,
            Inline::Tab(r) | Inline::Break(_, r) => r.hidden,
            Inline::Bookmark(_)
            | Inline::Range(_)
            | Inline::ControlStart(_)
            | Inline::ControlEnd(_)
            | Inline::CommentRef { .. }
            | Inline::FieldBegin { .. }
            | Inline::FieldSep { .. }
            | Inline::FieldEnd { .. } => true,
            _ => false,
        })
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
        assert_eq!(lines_to_take(5, 1, false, true, false, true), 0);
        assert_eq!(lines_to_take(5, 1, false, true, false, false), 1);
    }

    #[test]
    fn page_3_widow_control_leaves_two_lines_for_the_next_page() {
        assert_eq!(lines_to_take(5, 4, false, true, false, true), 3);
        assert_eq!(lines_to_take(3, 2, false, true, false, true), 0);
        assert_eq!(lines_to_take(3, 2, false, false, false, true), 1);
    }

    #[test]
    fn page_6_keep_lines_moves_a_first_part_whole() {
        assert_eq!(lines_to_take(5, 4, false, true, true, false), 0);
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
        let other_num = crate::docx::ParaStyle { num: Some((2, 1)), ..item.clone() };
        assert_eq!(super::gap(&env, &item, 14.0, &other_num, 14.0), 0.0);
        let explicit = crate::docx::ParaStyle { before_auto: false, space_before: 12.0, ..item.clone() };
        assert_eq!(super::gap(&env, &item, 14.0, &explicit, 12.0), 12.0);
        let plain = crate::docx::ParaStyle { num: None, list: None, ..item.clone() };
        assert_eq!(super::gap(&env, &item, 14.0, &plain, 14.0), 14.0);
        let other_list = crate::docx::ParaStyle { list: Some(1), ..item.clone() };
        assert_eq!(super::gap(&env, &item, 14.0, &other_list, 14.0), 14.0);
    }

    #[test]
    fn para_5_without_html_spacing_the_explicit_spacing_applies_and_adds_in_lists_too() {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(crate::docx::Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let env = super::TextEnv { do_not_use_html_paragraph_auto_spacing: true, ..super::TextEnv::new(&doc) };
        let item =
            crate::docx::ParaStyle { before_auto: true, after_auto: true, space_before: 3.0, space_after: 2.0, list: Some(0), ..(*p.style).clone() };
        let (after, before) = (super::space_after(&env, &item), super::space_before(&env, &item));
        assert_eq!((after, before), (2.0, 3.0));
        assert_eq!(super::gap(&env, &item, after, &item, before), 5.0);
    }

    #[test]
    fn a_hard_break_takes_what_fits() {
        assert_eq!(lines_to_take(5, 2, true, true, true, true), 2);
    }
}
