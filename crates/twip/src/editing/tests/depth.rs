//! Deep nesting: a 100-level nested table, built in the test, survives opening, a click and
//! typing in the innermost cell, Tab there, undo of everything, and save and reopen, with integrity and dump checks
//! after each.

use serde_json::Value;

use super::super::Editor;
use super::super::commands::Command;
use super::super::input::PointerKind;
use super::super::inspect::inspect;
use super::super::keymap::Mods;
use super::{fonts, open};
use crate::docx::{Block, Document, Table};

const DEPTH: usize = 100;

/// FX-NEST-2's outer table reduced to one cell, nested `DEPTH` levels deep, the innermost cell holding "deep".
fn deep_document() -> Document {
    let base = open("FX-NEST-2.docx");
    let mut doc = base.document().clone();
    let Some(Block::Table(outer)) = doc.sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))).cloned() else {
        panic!("FX-NEST-2 has a table")
    };
    let para = |text: &str| {
        let Some(Block::Para(p)) = outer.rows[0].cells[0].blocks.first().cloned() else { panic!("a first paragraph") };
        let mut p = p;
        p.id = crate::docx::next_para_id();
        p.src = None;
        let run = p.mark.clone();
        p.inlines = vec![crate::docx::Inline::Text(text.to_string(), run)];
        p
    };
    let one_cell = |content: Vec<Block>| -> Table {
        let mut t = outer.clone();
        t.rows.truncate(1);
        t.rows[0].cells.truncate(1);
        t.rows[0].cells[0].span = 1;
        t.rows[0].cells[0].blocks = content;
        t.src = None;
        t.id = crate::docx::next_para_id();
        t
    };
    let mut t = one_cell(vec![Block::Para(para("deep"))]);
    for level in 1..DEPTH {
        t = one_cell(vec![Block::Para(para(&format!("level {}", DEPTH - level))), Block::Table(t), Block::Para(para(""))]);
    }
    doc.sections[0].blocks = vec![Block::Para(para("Before.")), Block::Table(t), Block::Para(para("After."))];
    doc
}

fn state(e: &mut Editor) -> Value {
    let st = inspect(e, "caret,dump,integrity", true);
    assert_eq!(st["integrity"], serde_json::json!([]), "integrity");
    st
}

#[test]
fn a_hundred_levels_of_nesting_edit_undo_save_and_reopen() {
    // The wasm build's stack (crates/twip-wasm/build.rs: STACK_SIZE=8MB); depth 100 needs under 1 MB natively.
    std::thread::Builder::new().stack_size(8 << 20).spawn(run).expect("thread").join().expect("no panic");
}

fn run() {
    let mut e = Editor::with_document(deep_document(), fonts());
    e.complete(fonts());
    let original = state(&mut e)["dump"].clone();
    let deep = original["paragraphs"].as_array().expect("paragraphs").iter().position(|p| p["text"] == "deep").expect("the innermost paragraph");
    assert_eq!(original["paragraphs"][deep]["depth"], DEPTH as u64);

    // Click on the innermost line when it is drawn wide enough to hold one; else place the caret there.
    let target = e
        .page_geometries()
        .into_iter()
        .find_map(|(page, g)| g.lines.iter().find(|l| l.chain.len() == DEPTH).map(|l| (page, l.x0() + 1.0, (l.top + l.bottom) / 2.0)));
    match target {
        Some((page, x, y)) => {
            let (x, y) = e.stack().to_doc(page, x, y).expect("on the stack");
            let msg = |seq: u64, kind: &str| super::super::input::Message {
                seq,
                input: super::super::input::Input::Pointer {
                    kind: if kind == "down" { PointerKind::Down } else { PointerKind::Up },
                    x,
                    y,
                    clicks: 1,
                    mods: Mods::default(),
                },
            };
            e.apply(msg(1, "down"), fonts()).expect("down");
            e.apply(msg(2, "up"), fonts()).expect("up");
        }
        None => {
            let id = crate::layout::ParaId(find_para(e.document(), "deep"));
            let p = super::super::position::Position::text(super::super::position::StoryId::Body, id, crate::layout::Unit(4));
            e.set_caret_for_benchmark(super::super::selection::Selection::Caret(p));
        }
    }
    let st = state(&mut e);
    assert_eq!(st["caret"]["chain"].as_array().map(Vec::len), Some(DEPTH), "the caret is in the innermost cell");

    for c in "xyz".chars() {
        e.execute(Command::InsertText(c.to_string()), fonts());
    }
    let typed = state(&mut e)["dump"].clone();
    assert!(typed["paragraphs"][deep]["text"].as_str().is_some_and(|t| t.contains("xyz")));

    // Tab in the innermost (only) cell adds a row to the innermost table only.
    e.execute(Command::NextCell { back: false }, fonts());
    let tabbed = state(&mut e)["dump"].clone();
    let rows = |d: &Value| d["tables"].as_array().map(|t| t.iter().map(|t| t["rows"].as_u64().unwrap_or(0)).collect::<Vec<_>>()).unwrap_or_default();
    let r = rows(&tabbed);
    assert_eq!(r.last(), Some(&2), "the innermost table has a new row");
    assert!(r[..r.len() - 1].iter().all(|&n| n == 1), "no other table changed");

    while e.history().undo_depth() > 0 {
        e.execute(Command::Undo, fonts());
        state(&mut e);
    }
    assert_eq!(state(&mut e)["dump"], original, "undo-all restores the document");
    while e.history().redo_depth() > 0 {
        e.execute(Command::Redo, fonts());
    }
    assert_eq!(state(&mut e)["dump"], tabbed, "redo-all restores the edits");

    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut again = Editor::open(&bytes, fonts()).expect("reopens");
    again.complete(fonts());
    assert_eq!(state(&mut again)["dump"], tabbed, "the saved document reopens as edited");
}

fn find_para(doc: &Document, text: &str) -> u64 {
    fn walk(blocks: &[Block], text: &str) -> Option<u64> {
        blocks.iter().find_map(|b| match b {
            Block::Para(p) => (super::super::text::UnitText::of(p).text == text).then_some(p.id),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).find_map(|c| walk(&c.blocks, text)),
        })
    }
    doc.sections.iter().find_map(|s| walk(&s.blocks, text)).expect("paragraph")
}
