//! Table regression tests whose expectations the scenario files can't state (borders, spans, dialogs, saving); the
//! related scenarios are tests/scenarios/ltb/struct/LTB-<id>.json.

use serde_json::json;

use crate::api::services::{DialogKind, DialogValues};
use crate::api::session::Session;
use crate::docx::props::Border;
use crate::docx::{Block, Table};
use crate::editing::Editor;
use crate::editing::commands::Command;
use crate::editing::inspect::inspect;
use crate::editing::position::{Position, StoryId};
use crate::editing::selection::Selection;
use crate::editing::tests::scenarios::{Run, paragraph};
use crate::editing::tests::{fonts, open, para_id, texts};
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

/// An engine with a session on fixture `name`, laid out in a 960 x 675 viewport.
fn engine(name: &str) -> (crate::api::engine::Engine, crate::api::engine::SessionId) {
    let (mut en, id) = super::session(&fixture(name));
    let s = en.session(id).expect("a session");
    s.host().set_size(960.0, 675.0, 1.0);
    s.with_editor(|e| e.complete(fonts()));
    (en, id)
}

/// The caret at unit `u` of the body's paragraph `n`, the state following it.
fn caret_in(s: &mut Session, n: usize, u: u32) {
    s.with_editor(|e| caret(e, n, u));
}

/// The Table Properties dialog's values for the selection.
fn table_properties(s: &mut Session) -> (crate::api::services::DialogModel, crate::api::services::TablePropertiesDialog) {
    let m = s.dialog_open(DialogKind::TableProperties).expect("the dialog opens in a cell");
    let DialogValues::TableProperties(v) = m.values.clone() else { panic!("{m:?}") };
    (m, v)
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
    r.step(&json!({"command": {"type": "setCellBorders", "edges": "all", "border": {"style": "double", "width": 1.5, "color": "#C00000"}}}));
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
    let cases = [(1, "right"), (1, "left"), (7, "left"), (7, "right"), (6, "right"), (24, "left"), (31, "right")];
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
        r.step(&json!({ "command": { "type": "insertColumns", "where": cmd } }));
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
    let outside =
        json!({ "command": { "type": "setCellBorders", "edges": "outside", "border": { "style": "double", "width": 1.5, "color": "#C00000" } } });
    // The 2-column span in row 4 (grid columns 4-5).
    let mut r = Run::open("FX-TBL-MERGED.docx", "LTB-18");
    r.step(&json!({ "click": { "para": 23, "offset": 2 } }));
    r.step(&outside);
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
    r.step(&outside);
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
/// The command a step of [`id_steps`] runs.
fn step_command(name: &str) -> Command {
    use crate::api::command::{CellBorders, CellShift, HorizontalSide, InsertColumns, InsertRows, SelectUnit, SplitCells, VerticalSide};
    use crate::api::value::{AutoOr, CellAlignment, Color, HeightRule, Points, RowHeight};
    match name {
        "table.insert-below" => Command::InsertRows(InsertRows { side: VerticalSide::Below, count: None }),
        "table.insert-above" => Command::InsertRows(InsertRows { side: VerticalSide::Above, count: None }),
        "table.insert-left" => Command::InsertColumns(InsertColumns { side: HorizontalSide::Left, count: None }),
        "table.insert-right" => Command::InsertColumns(InsertColumns { side: HorizontalSide::Right, count: None }),
        "table.delete.row" => Command::DeleteRows,
        "table.delete.column" => Command::DeleteColumns,
        "table.delete.cells-left" => Command::DeleteCells(CellShift::Left),
        "table.select.row" => Command::Select(SelectUnit::Row),
        "table.merge" => Command::MergeCells,
        "home.undo" => Command::Undo,
        "home.redo" => Command::Redo,
        "table.split" => Command::SplitCells(SplitCells { columns: 2, rows: None }),
        "table.split-rows" => Command::SplitCells(SplitCells { columns: 1, rows: Some(2) }),
        "table.distribute" => Command::DistributeColumns,
        "table.borders.all" => Command::SetCellBorders(CellBorders { edges: crate::api::command::BorderEdges::All, border: None }),
        "table.shading" => Command::SetCellShading(Some(Color(0xDDEBF7))),
        "table.row-height" => Command::SetRowHeight(AutoOr::Value(RowHeight { height: Points(30.0), rule: HeightRule::Exact })),
        "table.cell-align.center" => Command::SetCellAlignment(CellAlignment::Center),
        "table.header-row" => Command::SetHeaderRow(true),
        other => panic!("no step {other}"),
    }
}

fn id_steps(steps: &[&str]) -> Vec<String> {
    let (mut en, id) = engine("FX-TBL-BASIC.docx");
    let en = en.session(id).expect("a session");
    let outside = en.with_editor(|e| ids_outside(e, Some(1)));
    // The caret in the second table's cell (1,1), or the nearest cell left.
    let place = |en: &mut Session| {
        en.with_editor(|e| {
            let t = top_tables(e)[1];
            let row = &t.rows[1.min(t.rows.len() - 1)];
            let cell = &row.cells[1.min(row.cells.len() - 1)];
            let Some(Block::Para(p)) = cell.blocks.first() else { panic!("a cell starts with a paragraph") };
            let p = crate::layout::ParaId(p.id);
            e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, p, Unit(0))));
        })
    };
    let mut failures = Vec::new();
    for (k, &id) in steps.iter().enumerate() {
        // Commands that act on a selection keep the one before them.
        if !matches!(id, "table.merge" | "home.undo" | "home.redo") {
            place(en);
        }
        match id {
            "dialog-cancel" => drop(table_properties(en)),
            "dialog-apply" => {
                let (m, _) = table_properties(en);
                en.dialog_apply(m).expect("applies");
            }
            // Some steps aren't available where the one before left the selection.
            _ => drop(en.run(step_command(id))),
        }
        let st = en.with_editor(|e| {
            e.complete(fonts());
            inspect(e, "geometry,dump,integrity", true)
        });
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
        if en.with_editor(|e| ids_outside(e, Some(1))) != outside {
            failures.push(format!("{what}: paragraphs or tables outside the edited table have new ids"));
        }
    }
    failures
}

#[test]
fn ltb_22_every_command_leaves_caret_stops_in_the_model_and_untouched_ids() {
    // Every table command, a cancelled and an applied Table Properties dialog, undo
    // and redo.
    let steps: &[&str] = &[
        "table.insert-below",
        "table.insert-above",
        "table.insert-left",
        "table.insert-right",
        "table.delete.row",
        "table.delete.column",
        "table.select.row",
        "table.merge",
        "home.undo",
        "home.undo",
        "home.redo",
        "table.split",
        "table.split-rows",
        "table.distribute",
        "table.borders.all",
        "table.shading",
        "table.row-height",
        "table.cell-align.center",
        "table.header-row",
        "dialog-cancel",
        "dialog-apply",
        "home.undo",
        "home.redo",
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
    let failures = id_steps(&["table.split-rows", "table.delete.cells-left"]);
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
    let (mut en, id) = engine("FX-TBL-BASIC.docx");
    let en = en.session(id).expect("a session");
    caret_in(en, 13, 8);
    en.text(" typed");
    assert_eq!(en.with_editor(|e| texts(e))[13], "T2 R1 C1 typed");
    table_properties(en);
    // Cancel: the host sends nothing.
    en.run(Command::Undo).expect("undoes");
    assert_eq!(en.with_editor(|e| texts(e)), texts(&original), "undo gives the cell back");
    let bytes = en.export(&Default::default()).expect("saves after the undo").bytes.0;
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
    use crate::api::state::Tri;
    use crate::api::value::{AutoOr, HeightRule, Points, RowHeight};
    let (mut en, id) = engine("FX-REPEAT-REGION.docx");
    let en = en.session(id).expect("a session");
    let undo = |en: &mut Session| en.with_editor(|e| e.history().undo_depth());
    let heights =
        |en: &mut Session| -> Vec<Option<(f64, String)>> { en.with_editor(|e| top_tables(e)[0].rows.iter().map(|r| r.height.clone()).collect()) };
    // "Item 3" is row 3 (the header row, then Items 1-3, then the picture row).
    caret_in(en, 11, 2);
    let f = en.with_editor(|e| e.format_state()).expect("a state");
    assert_eq!(f.cell, Some((3, 0)), "the caret's row and cell");
    let before = undo(en);
    en.run(step_command("table.row-height")).expect("sets the height");
    assert_eq!(undo(en), before + 1, "a row height is one step");
    assert_eq!(heights(en), vec![None, None, None, Some((30.0, "exact".to_string())), None], "only the caret's row");
    let exact = Tri::Uniform(AutoOr::Value(RowHeight { height: Points(30.0), rule: HeightRule::Exact }));
    assert_eq!(table_properties(en).1.row_height, exact, "the dialog reads the caret's row");
    // "Item 2" is row 2, with no height.
    caret_in(en, 8, 2);
    assert_eq!(en.with_editor(|e| e.format_state()).expect("a state").cell, Some((2, 0)));
    let (mut m, mut v) = table_properties(en);
    assert!(!matches!(v.row_height, Tri::Uniform(AutoOr::Value(_))), "row 2 has no height: {:?}", v.row_height);
    // A width change through the dialog: one step.
    v.width = crate::api::value::TableWidth::Points { value: Points(400.0) };
    m.values = DialogValues::TableProperties(v);
    let before = undo(en);
    en.dialog_apply(m).expect("applies");
    assert_eq!(undo(en), before + 1, "the dialog's width change is one step");
    assert_eq!(en.with_editor(|e| top_tables(e)[0].width), crate::docx::props::Width::Dxa(400.0));
    // And a column width from the command bar: one step.
    let before = undo(en);
    en.run(Command::SetColumnWidth(Points(100.0))).expect("sets the width");
    assert_eq!(undo(en), before + 1, "a column width is one step");
    let heights = heights(en);
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
        json!({"command": {"type": "insertTable", "rows": 2, "columns": 2}}),
        json!({ "drag": { "from": { "para": 34, "offset": 2 }, "to": { "para": 38, "offset": 2 } } }),
        json!({"command": {"type": "mergeCells"}}),
        json!({ "click": { "para": 28, "offset": 2 } }),
        json!({"command": {"type": "insertColumns", "where": "right"}}),
        json!({ "click": { "para": 16, "offset": 2 } }),
        json!({"command": {"type": "insertRows", "where": "below"}}),
        json!({ "click": { "para": 13, "offset": 2 } }),
        json!({"command": {"type": "insertColumns", "where": "left"}}),
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
        e.execute(
            Command::InsertRows(crate::api::command::InsertRows {
                side: if true { crate::api::command::VerticalSide::Below } else { crate::api::command::VerticalSide::Above },
                count: Some(1),
            }),
            fonts(),
        );
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
