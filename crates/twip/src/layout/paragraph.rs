//! Paragraph spacing (PARA-1 to PARA-5) and how many of a paragraph's lines a column takes (PAGE-3 widow/orphan
//! control, PAGE-6 keepLines), separate from the column fill so they can be tested alone.

use crate::docx::ParaStyle;

use super::env::TextEnv;

pub fn space_before(s: &ParaStyle) -> f64 {
    if s.before_auto { 14.0 } else { s.space_before }
}

pub fn space_after(s: &ParaStyle) -> f64 {
    if s.after_auto { 14.0 } else { s.space_after }
}

/// PARA-1 without contextual spacing.
pub fn gap_plain(env: &TextEnv, prev_after: f64, before: f64) -> f64 {
    if env.do_not_use_html_paragraph_auto_spacing { prev_after + before } else { prev_after.max(before) }
}

/// The space between two paragraphs (PARA-1, PARA-4).
pub fn gap(env: &TextEnv, prev: &ParaStyle, prev_after: f64, next: &ParaStyle, before: f64) -> f64 {
    if prev.contextual && next.contextual && prev.style_id == next.style_id {
        return 0.0;
    }
    gap_plain(env, prev_after, before)
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
    fn a_hard_break_takes_what_fits() {
        assert_eq!(lines_to_take(5, 2, true, true, true, true), 2);
    }
}
