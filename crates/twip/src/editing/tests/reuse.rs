//! Layout reuse over a fixed edit script: per edit, the paragraphs, rows and pages reused, moved and laid, and the
//! shaping and line cache misses. `TWIP_REUSE_OUT=file` writes the report, to compare two builds: the counts are
//! deterministic, so they must be equal

use std::fmt::Write;

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::{fonts, texts_ids};
use crate::api::command::{Command as C, DeleteOptions};
use crate::api::value::{Direction, Points};
use crate::layout::Unit;

fn misses(e: &Editor) -> (u64, u64) {
    let cache = e.layout.cache();
    (cache.shaped.counts().1, cache.lines.counts().1)
}

#[test]
fn layout_reuse_over_a_fixed_edit_script() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let files = ["text-basics.docx", "pagination.docx", "tables.docx", "lists.docx", "model/style-toggles.docx", "model/theme-references.docx"];
    let mut report = String::new();
    for name in files {
        let mut e = Editor::open(&std::fs::read(root.join(name)).expect("the fixture"), fonts()).expect("opens");
        e.complete(fonts());
        let _ = crate::prof::take_reuse();
        let ids = texts_ids(&e);
        let (a, b) = (ids[ids.len().min(2) - 1], ids[ids.len() / 2]);
        let script: Vec<(&str, Box<dyn Fn(&mut Editor) -> Outcome>)> = vec![
            (
                "type",
                Box::new(move |e| {
                    e.select_range(a, Unit(0), Unit(0), fonts());
                    e.execute(Command::InsertText("x".into()), fonts())
                }),
            ),
            (
                "bold",
                Box::new(move |e| {
                    e.select_range(b, Unit(0), Unit(u32::MAX), fonts());
                    e.execute(C::SetBold(true), fonts())
                }),
            ),
            ("size", Box::new(|e| e.execute(C::SetFontSize(Points(15.0)), fonts()))),
            (
                "enter",
                Box::new(move |e| {
                    e.select_range(b, Unit(1), Unit(1), fonts());
                    e.execute(C::InsertParagraph, fonts())
                }),
            ),
            ("backspace", Box::new(|e| e.execute(C::Delete(DeleteOptions { direction: Direction::Backward, unit: None }), fonts()))),
            ("undo", Box::new(|e| e.execute(Command::Undo(Default::default()), fonts()))),
            ("redo", Box::new(|e| e.execute(Command::Redo(Default::default()), fonts()))),
        ];
        for (step, f) in &script {
            let before = misses(&e);
            let out = f(&mut e);
            e.complete(fonts());
            let now = misses(&e);
            let changed = matches!(out, Outcome::Changed(_));
            writeln!(
                report,
                "{name} {step} changed={changed} reuse={:?} shaped={} lines={}",
                crate::prof::take_reuse(),
                now.0 - before.0,
                now.1 - before.1
            )
            .unwrap();
        }
    }
    assert!(report.lines().count() == files.len() * 7, "{report}");
    if let Some(out) = std::env::var_os("TWIP_REUSE_OUT") {
        std::fs::write(out, &report).expect("writes the report");
    }
}
