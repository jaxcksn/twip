//! Discarded attempts leave nothing behind.

use crate::docx::{Block, Inline};
use crate::layout::cache::LayoutCache;
use crate::layout::document::layout_records;
use crate::layout::fragment::{Child, PageChild};
use crate::layout::ids::ParaId;

use super::{fixture, fonts};

/// The paragraphs with lines on a page's bodies.
fn paras_drawn(page: &crate::layout::fragment::PageFragment) -> Vec<ParaId> {
    fn walk(children: &[Child], out: &mut Vec<ParaId>) {
        for c in children {
            match c {
                Child::Para(p) if !p.lines.is_empty() => out.push(p.para),
                Child::Box(b) => walk(&b.children, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for c in page.children.iter() {
        if let PageChild::Body(b) = c {
            walk(&b.children, &mut out);
        }
    }
    out
}

#[test]
fn discarded_attempt_leaves_no_effects() {
    // A keepNext heading at the bottom of page 1 moves to page 2 with the paragraph it keeps with.
    let Some(mut doc) = fixture(r"K1\K1b_keepnext.docx") else { return };
    let heading = doc.sections[0].blocks.iter().position(|b| matches!(b, Block::Para(p) if p.style.keep_next)).expect("K1b has a keepNext heading");
    let Block::Para(p) = &mut doc.sections[0].blocks[heading] else { unreachable!() };
    p.inlines.insert(0, Inline::Bookmark("moved".into()));
    let id = ParaId(p.id);
    let (snapshot, records) = layout_records(&doc, &fonts(), &mut LayoutCache::new());
    let effects: Vec<_> = records.iter().map(|r| &r.effects).collect();
    assert!(snapshot.pages().len() >= 2);
    // The chain was placed on page 1 first, then rewound: nothing of it stays there.
    assert!(!paras_drawn(&snapshot.pages()[0]).contains(&id), "the rewound heading's lines stayed on page 1");
    assert!(!effects[0].paras.contains(&id), "page 1 recorded the rewound heading");
    assert!(!effects[0].bookmarks.iter().any(|b| b == "moved"), "page 1 recorded the rewound heading's bookmark");
    assert!(paras_drawn(&snapshot.pages()[1]).contains(&id));
    assert!(effects[1].bookmarks.iter().any(|b| b == "moved"));
}
