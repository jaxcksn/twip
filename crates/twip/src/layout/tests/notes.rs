//! A note moves with its reference when a keep-with-next chain is rewound (discarded attempts discard their
//! effects).

use crate::docx::model::next_para_id;
use crate::docx::{Block, Inline, NoteKind};
use crate::layout::cache::LayoutCache;
use crate::layout::document::layout_records;
use crate::layout::fragment::{BoxKind, Child, PageChild, PageFragment};

use super::{fixture, fonts};

/// Whether a page draws a box for footnote `id`.
fn shows_note(page: &PageFragment, id: i64) -> bool {
    fn walk(children: &[Child], id: i64) -> bool {
        children.iter().any(|c| match c {
            Child::Box(b) => matches!(b.kind, BoxKind::Note { kind: NoteKind::Foot, id: i } if i == id) || walk(&b.children, id),
            _ => false,
        })
    }
    page.children.iter().any(|c| matches!(c, PageChild::Body(b) if walk(&b.children, id)))
}

#[test]
fn rewound_note_follows_reference() {
    // A keepNext heading at the bottom of page 1 moves to page 2 with the paragraph it keeps with.
    const ID: i64 = 9001;
    let Some(mut doc) = fixture(r"K1\K1b_keepnext.docx") else { return };
    let blocks = &mut doc.sections[0].blocks;
    let heading = blocks.iter().position(|b| matches!(b, Block::Para(p) if p.style.keep_next)).expect("K1b has a keepNext heading");
    let Some(Block::Para(body)) = blocks.iter().find(|b| matches!(b, Block::Para(p) if !p.style.keep_next)).cloned() else { unreachable!() };
    let Block::Para(p) = &mut blocks[heading] else { unreachable!() };
    p.inlines.push(Inline::NoteRef { kind: NoteKind::Foot, id: ID, custom: false, run: p.mark.clone() });
    let mut note = body;
    note.inlines = vec![Inline::Text("The note's text.".into(), note.mark.clone())];
    note.id = next_para_id();
    doc.footnotes.insert(ID, vec![Block::Para(note)]);
    let (snapshot, records) = layout_records(&doc, &fonts(), &mut LayoutCache::new());
    let pages: Vec<usize> = snapshot.pages().iter().enumerate().filter(|(_, p)| shows_note(p, ID)).map(|(i, _)| i).collect();
    assert_eq!(pages, [1], "the note is drawn once, on page 2 with its reference");
    assert!(!records[0].effects.notes_shown.iter().any(|n| n.1 == ID), "page 1's effects don't show the rewound note");
    assert!(records[1].effects.notes_shown.iter().any(|n| (n.0, n.1) == (NoteKind::Foot, ID)), "page 2's effects show it");
}
