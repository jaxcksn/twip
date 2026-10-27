//! Any budget gives the same final layout (budgets are control inputs).

use crate::layout::Budget;

use super::fonts;
use super::random_edits::{documents, run_edits_with};

/// The same seeded edits, completed in runs of 1 unit, 7 units and without a budget, each equal a full layout after
/// every edit (so they equal each other).
#[test]
fn budget_independence() {
    let fonts = fonts();
    for (name, d) in documents().into_iter().take(4) {
        for budget in [Budget::Units(1), Budget::Units(7), Budget::Unlimited] {
            run_edits_with(&name, d.clone(), &fonts, 5, 8, budget);
        }
    }
}
