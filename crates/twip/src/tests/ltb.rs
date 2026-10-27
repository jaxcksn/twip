//! Table regression tests whose expectations the scenario files can't state (borders, spans, dialogs, saving); the
//! related scenarios are tests/scenarios/ltb/struct/LTB-<id>.json.

use serde_json::{Map, Value, json};

use crate::docx::props::Border;
use crate::docx::{Block, Table};
use crate::editing::Editor;
use crate::editing::commands::Command;
use crate::editing::inspect::inspect;
use crate::editing::position::{Position, StoryId};
use crate::editing::selection::Selection;
use crate::editing::tests::scenarios::{Run, paragraph};
use crate::editing::tests::{fonts, open, para_id, texts};
use crate::host::Engine;
use crate::layout::Unit;

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
}

/// The body's top-level tables in order.
fn top_tables(e: &Editor) -> Vec<&Table> {
    e.document().sections.iter().flat_map(|s| s.blocks.iter()).filter_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).collect()
}

fn caret(e: &mut Editor, n: usize, u: u32) {
    let p = para_id(e, n);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, p, Unit(u))));
}

fn double(b: &Option<Border>) -> bool {
    b.as_ref().is_some_and(|b| b.style == "double")
}

/// Every paragraph and table id in the body, skipping the `skip`-th top-level table and everything in it.
fn ids_outside(e: &Editor, skip: Option<usize>) -> Vec<u64> {
    fn all(blocks: &[Block], out: &mut Vec<u64>) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push(p.id),
                Block::Table(t) => {
                    out.push(t.id);
                    t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| all(&c.blocks, out));
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut k = 0;
    for b in e.document().sections.iter().flat_map(|s| s.blocks.iter()) {
        match b {
            Block::Para(p) => out.push(p.id),
            Block::Table(_) => {
                if Some(k) != skip {
                    all(std::slice::from_ref(b), &mut out);
                }
                k += 1;
            }
        }
    }
    out
}

/// Rows whose grid offsets and spans don't add up to the table's grid.
fn overruns(t: &Table) -> Vec<String> {
    t.rows
        .iter()
        .enumerate()
        .filter_map(|(r, row)| {
            let w = row.grid_before + row.cells.iter().map(|c| c.span.max(1)).sum::<usize>() + row.grid_after;
            (w != t.grid.len()).then(|| format!("row {r} spans {w} of {}", t.grid.len()))
        })
        .collect()
}

fn engine(name: &str) -> Engine {
    let mut en = Engine::with_fonts(crate::font::Fonts::system());
    en.open(&fixture(name));
    en.viewport(0.0, 0.0, 960.0, 675.0, 1.0);
    while en.idle() {}
    en
}

/// A dialog's field values and hidden state, as `dialog_apply` takes them back.
fn dialog_values(open: &str) -> Map<String, Value> {
    let v: Value = serde_json::from_str(open).unwrap_or_else(|e| panic!("dialog JSON {open}: {e}"));
    let mut m = Map::new();
    for f in v["dialog"]["rows"].as_array().into_iter().flatten().flat_map(|r| r.as_array().cloned().unwrap_or_default()) {
        if let (Some(id), Some(x)) = (f["id"].as_str(), f["value"].as_str()) {
            m.insert(id.into(), x.into());
        }
    }
    for (k, x) in v["dialog"]["state"].as_object().into_iter().flatten() {
        m.insert(k.clone(), x.clone());
    }
    m
}

fn num(m: &Map<String, Value>, k: &str) -> f64 {
    m.get(k).and_then(Value::as_str).and_then(|s| s.trim().parse().ok()).unwrap_or_else(|| panic!("dialog field {k}: {m:?}"))
}

#[test]
fn ltb_11_borders_on_a_mixed_depth_selection_change_the_outer_table_only() {
    // From an inner cell into another outer cell is an outer cell block; Borders
    // apply to the outer cells, and no inner table's borders change.
    fn inner(e: &Editor) -> Vec<String> {
        fn walk(blocks: &[Block], depth: usize, out: &mut Vec<String>) {
            for b in blocks {
                if let Block::Table(t) = b {
                    if depth > 0 {
                        out.push(format!("table {:?}", t.borders));
                        for c in t.rows.iter().flat_map(|r| r.cells.iter()) {
                            out.push(format!("  cell {:?} {:?} {:?} {:?}", c.top, c.left, c.bottom, c.right));
                        }
                    }
                    t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, depth + 1, out));
                }
            }
        }
        let mut out = Vec::new();
        e.document().sections.iter().for_each(|s| walk(&s.blocks, 0, &mut out));
        out
    }
    let mut r = Run::open("FX-NEST-2.docx", "LTB-11");
    let before = inner(&r.e);
    r.step(&json!({ "drag": { "from": { "para": 2, "offset": 2 }, "to": { "para": 12, "offset": 3 } } }));
    r.check(&json!({ "selection": { "kind": "cells", "table": 0, "anchor": [0, 0], "focus": [1, 0] } }), "LTB-11 drag");
    r.step(&json!({ "command": "table.borders.all", "arg": "double,1.5,C00000" }));
    assert_eq!(inner(&r.e), before, "no inner table's borders change");
    let t = top_tables(&r.e)[0];
    for (row, col) in [(0, 0), (1, 0)] {
        let c = &t.rows[row].cells[col];
        assert!(double(&c.top) && double(&c.left) && double(&c.bottom) && double(&c.right), "outer ({row},{col}): {c:?}");
    }
}

#[test]
fn ltb_16_after_a_column_insert_every_row_spans_the_grid() {
    // The grid gains one column, every row's offsets and spans add up to it,
    // every row without offsets gains one cell, and spanning cells keep their spans.
    let cases = [
        (1, "table.insert-right"),
        (1, "table.insert-left"),
        (7, "table.insert-left"),
        (7, "table.insert-right"),
        (6, "table.insert-right"),
        (24, "table.insert-left"),
        (31, "table.insert-right"),
    ];
    let mut failures = Vec::new();
    for (para, cmd) in cases {
        let mut r = Run::open("FX-TBL-MERGED.docx", "LTB-16");
        let shape = |t: &Table| -> Vec<(bool, usize, Vec<usize>)> {
            t.rows
                .iter()
                .map(|row| {
                    let mut spans: Vec<usize> = row.cells.iter().map(|c| c.span.max(1)).filter(|s| *s > 1).collect();
                    spans.sort();
                    (row.grid_before == 0 && row.grid_after == 0, row.cells.len(), spans)
                })
                .collect()
        };
        let before = shape(top_tables(&r.e)[0]);
        assert!(overruns(top_tables(&r.e)[0]).is_empty(), "the fixture's rows span its grid");
        r.step(&json!({ "click": { "para": para, "offset": 1 } }));
        r.step(&json!({ "command": cmd }));
        let t = top_tables(&r.e)[0];
        let mut why = overruns(t);
        if t.grid.len() != 7 {
            why.push(format!("grid has {} columns", t.grid.len()));
        }
        for (k, ((plain, n, spans), (_, n2, spans2))) in before.iter().zip(shape(t)).enumerate() {
            if *plain && n2 != n + 1 {
                let row = &t.rows[k];
                why.push(format!("row {k} has {n2} cells for {} (gridBefore {}, gridAfter {})", n + 1, row.grid_before, row.grid_after));
            }
            if *plain && spans2 != *spans {
                why.push(format!("row {k} spans {spans2:?} for {spans:?}"));
            }
        }
        if !why.is_empty() {
            failures.push(format!("{cmd} from paragraph {para}: {}", why.join("; ")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn ltb_18_outside_borders_reach_every_neighbour_across_spans_and_grid_offsets() {
    // A block side also sets the facing side of every cell beyond it, found by grid column
    // (spans, gridBefore and gridAfter), not by cell index.
    let arg = "double,1.5,C00000";
    // The 2-column span in row 4 (grid columns 4-5).
    let mut r = Run::open("FX-TBL-MERGED.docx", "LTB-18");
    r.step(&json!({ "click": { "para": 23, "offset": 2 } }));
    r.step(&json!({ "command": "table.borders.outside", "arg": arg }));
    let t = top_tables(&r.e)[0];
    let c = &t.rows[4].cells[3];
    assert!(double(&c.top) && double(&c.left) && double(&c.bottom) && double(&c.right), "the span itself: {c:?}");
    assert!(double(&t.rows[3].cells[3].bottom) && double(&t.rows[3].cells[4].bottom), "both cells above the span (columns 4 and 5)");
    assert!(!double(&t.rows[3].cells[2].bottom), "the cell above column 3 is not beside the span");
    assert!(double(&t.rows[4].cells[2].right), "the cell left of the span");
    assert!(double(&t.rows[5].cells[3].top), "the cell below at column 4 (row 5 starts at column 1, gridBefore 1)");
    assert!(!double(&t.rows[5].cells[2].top), "row 5's cell at column 3 is not below the span");
    // Row 5's last cell (grid column 4, gridAfter 1 beyond it).
    let mut r = Run::open("FX-TBL-MERGED.docx", "LTB-18");
    r.step(&json!({ "click": { "para": 27, "offset": 0 } }));
    r.step(&json!({ "command": "table.borders.outside", "arg": arg }));
    let t = top_tables(&r.e)[0];
    let c = &t.rows[5].cells[3];
    assert!(double(&c.top) && double(&c.left) && double(&c.bottom) && double(&c.right), "the cell itself: {c:?}");
    assert!(double(&t.rows[4].cells[3].bottom), "the span above (columns 4-5)");
    assert!(double(&t.rows[6].cells[1].top), "the cell below at column 4 (row 6: gridBefore 2, a span over columns 2-3)");
    assert!(!double(&t.rows[6].cells[0].top), "row 6's span over columns 2-3 is not below it");
    assert!(double(&t.rows[5].cells[2].right), "the cell to its left");
    assert!(overruns(t).is_empty(), "{:?}", overruns(t));
}

/// Runs host commands on FX-TBL-BASIC's second table, the caret put back in its cell (1,1)
/// before each (but merge, undo and redo, which take the selection before them); after each, every drawn caret stop
/// maps to a model paragraph and position, the integrity check holds, and nothing outside the edited table was
/// re-read (its paragraphs and tables keep their ids). `dialog-cancel` opens Table Properties and cancels it;
/// `dialog-apply` applies it as opened.
fn id_steps(steps: &[(&str, Option<&str>)]) -> Vec<String> {
    let mut en = engine("FX-TBL-BASIC.docx");
    let outside = ids_outside(en.editor().unwrap(), Some(1));
    // The caret in the second table's cell (1,1), or the nearest cell left.
    let place = |en: &mut Engine| {
        let e = en.editor().unwrap();
        let t = top_tables(e)[1];
        let row = &t.rows[1.min(t.rows.len() - 1)];
        let cell = &row.cells[1.min(row.cells.len() - 1)];
        let Some(Block::Para(p)) = cell.blocks.first() else { panic!("a cell starts with a paragraph") };
        let p = crate::layout::ParaId(p.id);
        e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, p, Unit(0))));
    };
    let mut failures = Vec::new();
    for (k, &(id, arg)) in steps.iter().enumerate() {
        // Commands that act on a selection keep the one before them.
        if !matches!(id, "table.merge" | "home.undo" | "home.redo") {
            place(&mut en);
        }
        match id {
            "dialog-cancel" => assert_ne!(en.dialog_open("TableProperties"), "null"),
            "dialog-apply" => {
                let v = dialog_values(&en.dialog_open("TableProperties"));
                en.dialog_apply("TableProperties", &Value::Object(v).to_string());
            }
            _ => {
                en.command(id, arg);
            }
        }
        while en.idle() {}
        let e = en.editor().unwrap();
        let st = inspect(e, "geometry,dump,integrity", true);
        let what = format!("step {k} {id}");
        if st["integrity"] != json!([]) {
            failures.push(format!("{what}: integrity {}", st["integrity"]));
        }
        for l in st["geometry"]["lines"].as_array().into_iter().flatten() {
            let Some(n) = l["para"].as_u64() else {
                failures.push(format!("{what}: a drawn line's paragraph isn't in the model: {l}"));
                break;
            };
            let len = paragraph(&st["dump"], n as usize)["text"].as_str().map_or(0, |t| t.chars().count()) as u64;
            if l["stops"].as_array().into_iter().flatten().any(|s| s[1].as_u64().is_none_or(|u| u > len)) {
                failures.push(format!("{what}: paragraph {n} ({len} units) has a stop past its end: {}", l["stops"]));
            }
        }
        if ids_outside(e, Some(1)) != outside {
            failures.push(format!("{what}: paragraphs or tables outside the edited table have new ids"));
        }
    }
    failures
}

#[test]
fn ltb_22_every_command_leaves_caret_stops_in_the_model_and_untouched_ids() {
    // Every table command, a cancelled and an applied Table Properties dialog, undo
    // and redo.
    let steps: &[(&str, Option<&str>)] = &[
        ("table.insert-below", None),
        ("table.insert-above", None),
        ("table.insert-left", None),
        ("table.insert-right", None),
        ("table.delete.row", None),
        ("table.delete.column", None),
        ("table.select.row", None),
        ("table.merge", None),
        ("home.undo", None),
        ("home.undo", None),
        ("home.redo", None),
        ("table.split", Some("2")),
        ("table.split-rows", Some("2")),
        ("table.distribute", None),
        ("table.borders.all", None),
        ("table.shading", Some("#DDEBF7")),
        ("table.row-height", Some("30,exact")),
        ("table.cell-align.center", None),
        ("table.header-row", None),
        ("dialog-cancel", None),
        ("dialog-apply", None),
        ("home.undo", None),
        ("home.redo", None),
    ];
    let failures = id_steps(steps);
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
}

#[test]
fn ltb_22_delete_cells_after_split_rows_keeps_the_grid_valid() {
    // The command list, Delete Cells included (every vertical merge keeps a start).
    let failures = id_steps(&[("table.split-rows", Some("2")), ("table.delete.cells-left", None)]);
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
}

#[test]
fn ltb_23_undo_after_a_cancelled_table_properties_dialog_saves() {
    // Type in a cell, open Table Properties and cancel it, undo, save: the file
    // reopens as the fixture was.
    let original = open("FX-TBL-BASIC.docx");
    let mut en = engine("FX-TBL-BASIC.docx");
    caret(en.editor().unwrap(), 13, 8);
    en.text(" typed");
    assert_eq!(texts(en.editor().unwrap())[13], "T2 R1 C1 typed");
    assert_ne!(en.dialog_open("TableProperties"), "null", "the dialog opens in a cell");
    // Cancel: the host sends nothing.
    en.command("home.undo", None);
    while en.idle() {}
    assert_eq!(texts(en.editor().unwrap()), texts(&original), "undo gives the cell back");
    let bytes = en.save().expect("saves after the undo");
    let mut again = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(texts(&again), texts(&original));
    let mut original = original;
    let tables = |e: &mut Editor| inspect(e, "dump", true)["dump"]["tables"].clone();
    assert_eq!(tables(&mut again), tables(&mut original));
}

#[test]
fn ltb_24_table_properties_read_and_change_the_caret_row_in_a_repeating_section() {
    // Rows wrapped in content controls (FX-REPEAT-REGION rows 1-3) count as rows;
    // the state for the caret's row reports that row and its height; each property change is one undo step.
    let mut en = engine("FX-REPEAT-REGION.docx");
    // "Item 3" is row 3 (the header row, then Items 1-3, then the picture row).
    caret(en.editor().unwrap(), 11, 2);
    let f = en.editor().unwrap().format_state().expect("a state");
    assert_eq!(f.cell, Some((3, 0)), "the caret's row and cell");
    let undo = en.editor().unwrap().history().undo_depth();
    en.command("table.row-height", Some("30,exact"));
    while en.idle() {}
    assert_eq!(en.editor().unwrap().history().undo_depth(), undo + 1, "a row height is one step");
    let heights: Vec<Option<(f64, String)>> = top_tables(en.editor().unwrap())[0].rows.iter().map(|r| r.height.clone()).collect();
    assert_eq!(heights, vec![None, None, None, Some((30.0, "exact".to_string())), None], "only the caret's row");
    let v = dialog_values(&en.dialog_open("TableProperties"));
    assert_eq!((num(&v, "rowHeight"), v.get("rule").and_then(Value::as_str)), (30.0, Some("Exactly")), "the dialog reads the caret's row");
    // "Item 2" is row 2, with no height.
    caret(en.editor().unwrap(), 8, 2);
    assert_eq!(en.editor().unwrap().format_state().expect("a state").cell, Some((2, 0)));
    let mut v = dialog_values(&en.dialog_open("TableProperties"));
    assert_eq!(num(&v, "rowHeight"), 0.0, "row 2 has no height");
    // A width change through the dialog: one step.
    v.insert("widthKind".into(), "Weight".into());
    v.insert("width".into(), "400".into());
    let undo = en.editor().unwrap().history().undo_depth();
    en.dialog_apply("TableProperties", &Value::Object(v).to_string());
    while en.idle() {}
    assert_eq!(en.editor().unwrap().history().undo_depth(), undo + 1, "the dialog's width change is one step");
    assert_eq!(top_tables(en.editor().unwrap())[0].width, crate::docx::props::Width::Dxa(400.0));
    // And a column width from the command bar: one step.
    let undo = en.editor().unwrap().history().undo_depth();
    en.command("table.column-width", Some("100"));
    while en.idle() {}
    assert_eq!(en.editor().unwrap().history().undo_depth(), undo + 1, "a column width is one step");
    let heights: Vec<Option<(f64, String)>> = top_tables(en.editor().unwrap())[0].rows.iter().map(|r| r.height.clone()).collect();
    assert_eq!(heights[3], Some((30.0, "exact".to_string())), "row 3 keeps its height");
    assert_eq!(heights[2], None, "row 2 still has none");
}

#[test]
fn ltb_35_structural_table_edits_save_and_reopen_to_the_same_dump() {
    // Rows, columns, a merge and a new nested table, made in the model, are written
    // and read back as the same dump and the same grid structure.
    fn structure(e: &Editor) -> Vec<String> {
        fn walk(blocks: &[Block], out: &mut Vec<String>) {
            for b in blocks {
                if let Block::Table(t) = b {
                    out.push(format!("table grid {:?}", t.grid.iter().map(|w| (w * 20.0).round()).collect::<Vec<_>>()));
                    for r in &t.rows {
                        let cells: Vec<String> = r.cells.iter().map(|c| format!("{}{:?}", c.span, c.v_merge)).collect();
                        out.push(format!("  row before {} after {} {}", r.grid_before, r.grid_after, cells.join(" ")));
                        r.cells.iter().for_each(|c| walk(&c.blocks, out));
                    }
                }
            }
        }
        let mut out = Vec::new();
        e.document().sections.iter().for_each(|s| walk(&s.blocks, &mut out));
        out
    }
    let mut r = Run::open("FX-TBL-BASIC.docx", "LTB-35");
    // Back to front, so the earlier paragraphs keep their numbers.
    let steps = [
        json!({ "click": { "para": 46, "offset": 2 } }),
        json!({ "command": "insert.table", "arg": "2,2" }),
        json!({ "drag": { "from": { "para": 34, "offset": 2 }, "to": { "para": 38, "offset": 2 } } }),
        json!({ "command": "table.merge" }),
        json!({ "click": { "para": 28, "offset": 2 } }),
        json!({ "command": "table.insert-right" }),
        json!({ "click": { "para": 16, "offset": 2 } }),
        json!({ "command": "table.insert-below" }),
        json!({ "click": { "para": 13, "offset": 2 } }),
        json!({ "command": "table.insert-left" }),
    ];
    for s in &steps {
        r.step(s);
    }
    assert_eq!(r.e.history().undo_depth(), 5, "five edits");
    let dump = |e: &mut Editor| {
        let d = inspect(e, "dump", true)["dump"].clone();
        json!({ "paragraphs": d["paragraphs"], "tables": d["tables"] })
    };
    let edited = dump(&mut r.e);
    let shape = structure(&r.e);
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-TBL-BASIC.ltb35.docx"), &bytes).expect("writes");
    }
    let mut again = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(structure(&again), shape);
    assert_eq!(dump(&mut again), edited);
}

/// A fixture with `n` paragraphs of text added at the body's end (a large document around the same tables).
fn with_paragraphs(name: &str, n: usize) -> Vec<u8> {
    use std::io::{Read, Write};
    let src = fixture(name);
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let paras: String = (0..n)
        .map(|k| format!("<w:p><w:r><w:t xml:space=\"preserve\">Added paragraph {k}: {}</w:t></w:r></w:p>", "lorem ipsum dolor sit amet ".repeat(12)))
        .collect();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "word/document.xml" {
            let mut s = String::from_utf8(data).unwrap();
            let at = s.rfind("<w:sectPr").expect("a body sectPr");
            s.insert_str(at, &paras);
            data = s.into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

#[test]
fn ltb_36_a_table_steps_undo_memory_follows_the_table_and_ids_survive() {
    // Insert Row's undo step costs the same in a small document and in one with
    // 3000 more paragraphs; every paragraph and table it didn't touch keeps its id through the step, undo and redo.
    let all = |e: &Editor| ids_outside(e, None);
    let insert = |e: &mut Editor| -> usize {
        caret(e, 16, 2);
        let b = e.history().bytes();
        e.execute(Command::InsertRows { below: true, n: 1 }, fonts());
        e.history().bytes() - b
    };
    let mut small = open("FX-TBL-BASIC.docx");
    let mut big = Editor::open(&with_paragraphs("FX-TBL-BASIC.docx", 3000), fonts()).expect("opens");
    assert_eq!(texts(&big)[16], "T2 R2 C1");
    assert!(texts(&big).len() > 3000);
    let ids0 = all(&big);
    let (s, b) = (insert(&mut small), insert(&mut big));
    assert!(b <= s + s / 4 + 256, "undo memory {b} bytes in the large document, {s} in the small one");
    let ids1 = all(&big);
    let missing = |now: &[u64]| ids0.iter().filter(|id| !now.contains(id)).count();
    assert_eq!(missing(&ids1), 0, "untouched ids survive the step");
    big.execute(Command::Undo, fonts());
    assert_eq!(all(&big), ids0, "undo gives every id back");
    big.execute(Command::Redo, fonts());
    assert_eq!(missing(&all(&big)), 0, "untouched ids survive redo");
}
