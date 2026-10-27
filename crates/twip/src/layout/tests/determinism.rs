//! Equal inputs give equal layouts: laying a document out twice, and with a fresh font catalog,
//! gives the same dump.

use crate::layout::dump::page_text;
use crate::layout::layout;

use super::{fonts, gate_set};

#[test]
fn layouts_are_deterministic() {
    let docs = gate_set(&[]);
    if docs.is_empty() {
        return;
    }
    let (f1, f2) = (fonts(), fonts());
    for (path, doc) in docs.iter().take(200) {
        let a = page_text(&layout(doc, &f1), doc);
        let b = page_text(&layout(doc, &f1), doc);
        let c = page_text(&layout(doc, &f2), doc);
        assert!(a == b, "{}: two layouts differ", path.display());
        assert!(a == c, "{}: a fresh font catalog gives a different layout", path.display());
    }
}
