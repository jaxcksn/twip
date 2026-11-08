//! Budget equivalence

use crate::layout::Budget;

use super::fonts;
use super::random_edits::{documents, run_edits_with};

#[test]
fn budget_independence() {
    let fonts = fonts();
    for (name, d) in documents().into_iter().take(4) {
        for budget in [Budget::Units(1), Budget::Units(7), Budget::Unlimited] {
            run_edits_with(&name, d.clone(), &fonts, 5, 8, budget);
        }
    }
}
