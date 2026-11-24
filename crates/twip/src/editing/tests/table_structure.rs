//! Table structure edits

use std::panic::{AssertUnwindSafe, catch_unwind};

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::super::revisions::resolve::Scope;
use super::super::selection::{CellRef, CellShape, Selection};
use super::{blank_body, fonts, para_id, texts};
use crate::api::command::{CellShift, SplitCells, VerticalDirection};
use crate::docx::{Block, Table, VMerge};
use crate::layout::Unit;

fn cmd(j: serde_json::Value) -> Command {
    serde_json::from_value(j).expect("a command")
}

fn cell(span: usize, extra: &str, text: &str) -> String {
    let gs = if span > 1 { format!("<w:gridSpan w:val=\"{span}\"/>") } else { String::new() };
    format!("<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/>{gs}{extra}</w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>", 1440 * span)
}

fn tc(t: &str, extra: &str) -> String {
    cell(1, extra, t)
}

fn tbl(grid: usize, rows: &[String]) -> String {
    let g: String = (0..grid).map(|_| "<w:gridCol w:w=\"1440\"/>").collect();
    format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"{}\" w:type=\"dxa\"/><w:tblLayout w:type=\"fixed\"/></w:tblPr><w:tblGrid>{g}</w:tblGrid>{}</w:tbl>",
        1440 * grid,
        rows.concat()
    )
}

fn row(cells: &[String]) -> String {
    format!("<w:tr>{}</w:tr>", cells.concat())
}

fn tr(pr: &str, cells: &[String]) -> String {
    format!("<w:tr><w:trPr>{pr}</w:trPr>{}</w:tr>", cells.concat())
}

fn open_xml(body: &str) -> Editor {
    Editor::open(&blank_body(body), fonts()).expect("opens")
}

/// An autofit table over `grid` columns, then a paragraph.
fn editor(grid: usize, rows: &str) -> Editor {
    let g: String = (0..grid).map(|_| "<w:gridCol w:w=\"1440\"/>").collect();
    open_xml(&format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid>{g}</w:tblGrid>{rows}</w:tbl><w:p><w:r><w:t>end</w:t></w:r></w:p>"
    ))
}

fn table(e: &Editor) -> &Table {
    e.document().sections[0].blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).expect("a table")
}

fn spans(e: &Editor) -> Vec<Vec<usize>> {
    table(e).rows.iter().map(|r| r.cells.iter().map(|c| c.span).collect()).collect()
}

fn cell_text(t: &Table, r: usize, c: usize) -> String {
    match &t.rows[r].cells[c].blocks[0] {
        Block::Para(p) => super::UnitText::of(p).text,
        _ => String::new(),
    }
}

fn caret(e: &mut Editor, para: usize) {
    let p = para_id(e, para);
    e.select_between((p, Unit(0)), (p, Unit(0)), fonts());
}

fn caret_in(e: &mut Editor, r: usize, c: usize) {
    let id = match &table(e).rows[r].cells[c].blocks[0] {
        Block::Para(p) => crate::layout::ParaId(p.id),
        _ => unreachable!(),
    };
    e.select_between((id, Unit(0)), (id, Unit(0)), fonts());
}

fn cells(e: &mut Editor, shape: CellShape, a: (usize, usize), f: (usize, usize)) {
    let table = crate::layout::TableId(table(e).id);
    e.set_selection(Selection::Cells { table, anchor: CellRef { row: a.0, col: a.1 }, focus: CellRef { row: f.0, col: f.1 }, shape });
}

fn violations(e: &Editor) -> Vec<String> {
    super::super::integrity::check(e.document(), e.index(), &e.selection).iter().map(|v| format!("{}: {}", v.invariant, v.detail)).collect()
}

/// Every vertical merge continues a merged cell directly above it.
fn chains_hold(t: &Table) -> bool {
    let g = crate::editing::table::Grid::of(t);
    (0..t.rows.len()).all(|r| {
        g.rows[r].iter().all(|&(c, s, v)| {
            v != VMerge::Continue
                || r.checked_sub(1).and_then(|a| g.cell_at(a, c).map(|k| (a, k))).is_some_and(|(a, k)| {
                    let (ac, asp, av) = g.rows[a][k];
                    ac == c && asp == s && av != VMerge::None
                })
        })
    })
}

const END: &str = "<w:p><w:r><w:t>end</w:t></w:r></w:p>";

#[test]
fn inserting_a_column_inside_a_merged_cell_widens_it() {
    for (para, side) in [(2, "right"), (3, "left")] {
        let body =
            format!("{}{END}", tbl(3, &[row(&[cell(2, "", "M"), cell(1, "", "C2")]), row(&[cell(1, "", "A"), cell(1, "", "B"), cell(1, "", "C")])]));
        let mut e = open_xml(&body);
        caret(&mut e, para);
        e.execute(cmd(serde_json::json!({"type":"insertColumns","where":side})), fonts());
        assert_eq!(spans(&e), vec![vec![3, 1], vec![1, 1, 1, 1]], "side {side}");
        assert_eq!(table(&e).grid.len(), 4);
    }
}

#[test]
fn commands_on_a_short_grid_do_not_panic() {
    let body = || format!("{}{END}", tbl(0, &[row(&[cell(1, "", "A"), cell(1, "", "B"), cell(1, "", "C")])]));
    let setup = || {
        let e = open_xml(&body());
        assert!(table(&e).grid.len() < 3, "grid is short");
        e
    };
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut e = setup();
        caret(&mut e, 2);
        e.execute(cmd(serde_json::json!({"type":"deleteColumns"})), fonts());
        assert_eq!(table(&e).rows[0].cells.len(), 2);
    }));
    assert!(r.is_ok(), "delete columns panicked");
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut e = setup();
        caret(&mut e, 0);
        e.execute(cmd(serde_json::json!({"type":"splitCells","columns":2})), fonts());
        assert_eq!(table(&e).rows[0].cells.len(), 4);
    }));
    assert!(r.is_ok(), "split cells panicked");
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut e = setup();
        cells(&mut e, CellShape::Rect, (0, 0), (0, 2));
        e.execute(cmd(serde_json::json!({"type":"distributeColumns"})), fonts());
    }));
    assert!(r.is_ok(), "distribute columns panicked");
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut e = setup();
        cells(&mut e, CellShape::Rect, (0, 2), (0, 2));
        e.copy_cells(1);
    }));
    assert!(r.is_ok(), "copy cells panicked");
}

#[test]
fn inserting_a_column_skips_a_row_with_no_cells() {
    let body = format!("{}{END}", tbl(2, &[row(&[cell(1, "", "A"), cell(1, "", "B")]), "<w:tr/>".into()]));
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut e = open_xml(&body);
        caret(&mut e, 0);
        e.execute(cmd(serde_json::json!({"type":"insertColumns","where":"right"})), fonts());
        assert_eq!(table(&e).rows[0].cells.len(), 3);
    }));
    assert!(r.is_ok(), "insert columns panicked");
}

#[test]
fn a_cell_selection_past_accepted_rows_is_clamped() {
    let del = "<w:trPr><w:del w:id=\"9\" w:author=\"a\" w:date=\"2020-01-01T00:00:00Z\"/></w:trPr>";
    let rows = [
        row(&[cell(1, "", "a"), cell(1, "", "b")]),
        row(&[cell(1, "", "c"), cell(1, "", "d")]),
        format!("<w:tr>{del}{}{}</w:tr>", cell(1, "", "e"), cell(1, "", "f")),
    ];
    let body = format!("{}{END}", tbl(2, &rows));
    for which in ["insertRows", "mergeCells", "distributeRows", "moveRows", "setRowProperties"] {
        let r = catch_unwind(AssertUnwindSafe(|| {
            let mut e = open_xml(&body);
            cells(&mut e, CellShape::Rect, (0, 0), (2, 1));
            e.resolve_revisions(true, &Scope::All, fonts());
            assert_eq!(table(&e).rows.len(), 2, "row accepted away");
            let j = match which {
                "insertRows" => serde_json::json!({"type":which,"where":"below"}),
                "moveRows" => serde_json::json!({"type":which,"value":"up"}),
                "setRowProperties" => serde_json::json!({"type":which,"hidden":true}),
                _ => serde_json::json!({"type":which}),
            };
            e.execute(cmd(j), fonts());
        }));
        assert!(r.is_ok(), "{which} panicked");
    }
}

#[test]
fn insert_table_honours_rows_above_63() {
    let mut e = open_xml("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    e.execute(cmd(serde_json::json!({"type":"insertTable","rows":100,"columns":2})), fonts());
    assert_eq!(table(&e).rows.len(), 100);
}

#[test]
fn a_new_row_does_not_inherit_a_tracked_deletion() {
    let del = "<w:trPr><w:del w:id=\"9\" w:author=\"a\" w:date=\"2020-01-01T00:00:00Z\"/></w:trPr>";
    let rows = [row(&[cell(1, "", "a")]), format!("<w:tr>{del}{}</w:tr>", cell(1, "", "b")), row(&[cell(1, "", "c")])];
    let mut e = open_xml(&format!("{}{END}", tbl(1, &rows)));
    assert!(!e.tracking());
    caret(&mut e, 1);
    e.execute(cmd(serde_json::json!({"type":"insertRows","where":"below"})), fonts());
    assert_eq!(table(&e).rows.len(), 4);
    e.resolve_revisions(true, &Scope::All, fonts());
    assert_eq!(table(&e).rows.len(), 3, "the user's new row survives accepting the old deletion");
}

fn three_by_two() -> Editor {
    let r = |a: &str, b: &str, c: &str| row(&[cell(1, "", a), cell(1, "", b), cell(1, "", c)]);
    open_xml(&format!("{}{END}", tbl(3, &[r("a", "b", "c"), r("d", "e", "f")])))
}

fn cell_marks(t: &Table) -> Vec<Vec<Option<crate::docx::review::CellMarkKind>>> {
    t.rows.iter().map(|r| r.cells.iter().map(|c| c.rev.structural.as_ref().map(|s| s.1.clone())).collect()).collect()
}

#[test]
fn a_tracked_column_deletion_marks_its_cells_deleted() {
    use crate::docx::review::CellMarkKind::Delete;
    let mut e = three_by_two();
    e.set_tracking(true);
    let before = texts(&e);
    caret(&mut e, 1);
    e.execute(cmd(serde_json::json!({"type":"deleteColumns"})), fonts());
    assert_eq!(cell_marks(table(&e)), vec![vec![None, Some(Delete), None]; 2]);
    assert_eq!(table(&e).grid.len(), 3, "the column stays until the deletion is accepted");
    let back = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(cell_marks(table(&back)), cell_marks(table(&e)), "saved as cellDel");
    let mut rejected = back;
    rejected.resolve_revisions(false, &Scope::All, fonts());
    assert_eq!(spans(&rejected), vec![vec![1, 1, 1]; 2], "column restored");
    assert_eq!(texts(&rejected), before, "text restored");
    e.resolve_revisions(true, &Scope::All, fonts());
    assert_eq!(spans(&e), vec![vec![1, 1]; 2]);
    assert_eq!(table(&e).grid.len(), 2, "accepting removes the column");
    assert_eq!((cell_text(table(&e), 0, 1), cell_text(table(&e), 1, 1)), ("c".into(), "f".into()));
}

#[test]
fn a_tracked_column_insertion_marks_its_cells_inserted() {
    use crate::docx::review::CellMarkKind::Insert;
    let mut e = three_by_two();
    e.set_tracking(true);
    caret(&mut e, 0);
    e.execute(cmd(serde_json::json!({"type":"insertColumns","where":"right"})), fonts());
    assert_eq!(cell_marks(table(&e)), vec![vec![None, Some(Insert), None, None]; 2]);
    e.resolve_revisions(false, &Scope::All, fonts());
    assert_eq!(spans(&e), vec![vec![1, 1, 1]; 2], "rejecting removes the column");
    assert_eq!(table(&e).grid.len(), 3);
}

#[test]
fn insert_table_beside_a_table_survives_save_and_reopen() {
    let mut e = open_xml(&format!("{}<w:p/>", tbl(1, &[row(&[cell(1, "", "a")])])));
    caret(&mut e, 1);
    e.execute(cmd(serde_json::json!({"type":"insertTable","rows":2,"columns":2})), fonts());
    let n = |e: &Editor| e.document().sections[0].blocks.iter().filter(|b| matches!(b, Block::Table(_))).count();
    assert_eq!(n(&e), 2, "two tables in the session");
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let back = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(n(&back), 2, "still two tables after reopen");
}

#[test]
fn merge_cells_on_a_long_row_is_linear() {
    // Re-measuring the grid for each cell took over half a minute at this size.
    let n = 6000;
    let cs: Vec<String> = (0..n).map(|_| cell(1, "", "x")).collect();
    let mut e = open_xml(&format!("{}{END}", tbl(n, &[row(&cs)])));
    cells(&mut e, CellShape::Rect, (0, n - 2), (0, n - 1));
    let t = std::time::Instant::now();
    e.execute(cmd(serde_json::json!({"type":"mergeCells"})), fonts());
    assert_eq!(table(&e).rows[0].cells.len(), n - 1);
    assert!(t.elapsed() < std::time::Duration::from_secs(15), "merged in {:?}", t.elapsed());
}

fn chained() -> Editor {
    let v = |m: &str, t: &str| cell(1, m, t);
    let rows = [
        row(&[v("<w:vMerge w:val=\"restart\"/>", "a"), v("", "b")]),
        row(&[v("<w:vMerge/>", ""), v("", "c")]),
        row(&[v("<w:vMerge/>", ""), v("", "d")]),
        row(&[v("", "x"), v("", "y")]),
    ];
    open_xml(&format!("{}{END}", tbl(2, &rows)))
}

fn merges(t: &Table) -> Vec<Vec<VMerge>> {
    t.rows.iter().map(|r| r.cells.iter().map(|c| c.v_merge).collect()).collect()
}

#[test]
fn splitting_a_merged_cell_into_rows_unmerges_it() {
    let mut e = chained();
    caret(&mut e, 0);
    let out = e.execute(cmd(serde_json::json!({"type":"splitCells","columns":1,"rows":2})), fonts());
    assert!(matches!(out, Outcome::Unchanged(_)), "two rows don't divide three: {out:?}");
    let out = e.execute(cmd(serde_json::json!({"type":"splitCells","columns":1,"rows":3})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = table(&e);
    assert_eq!(t.rows.len(), 4);
    assert!(merges(t).iter().all(|r| r.iter().all(|v| *v == VMerge::None)), "{:?}", merges(t));
}

#[test]
fn splitting_rows_beside_a_merged_cell_extends_it() {
    let mut e = chained();
    caret(&mut e, 1);
    let out = e.execute(cmd(serde_json::json!({"type":"splitCells","columns":1,"rows":2})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = table(&e);
    assert_eq!(t.rows.len(), 5);
    assert!(chains_hold(t), "{:?}", merges(t));
    assert_eq!(merges(t)[1], [VMerge::Continue, VMerge::None]);
}

#[test]
fn pasting_rows_inside_a_vertical_merge_splits_it() {
    let mut e = chained();
    cells(&mut e, CellShape::Rows, (3, 0), (3, 1));
    let f = e.copy_cells(1).expect("copies");
    caret_in(&mut e, 1, 0);
    e.paste_cells(&f, fonts());
    let t = table(&e);
    assert_eq!(t.rows.len(), 5);
    assert!(chains_hold(t), "{:?}", merges(t));
    assert_eq!(cell_text(t, 1, 0), "x", "the pasted row keeps its own cell");
    assert_eq!(t.rows[2].cells[0].v_merge, VMerge::Restart, "the merged cell's part below starts again");
}

#[test]
fn rows_holding_a_merge_pasted_inside_another_keep_the_merges_apart() {
    let mut e = chained();
    cells(&mut e, CellShape::Rows, (0, 0), (1, 1));
    let f = e.copy_cells(1).expect("copies");
    caret_in(&mut e, 1, 0);
    e.paste_cells(&f, fonts());
    let t = table(&e);
    let below = 1 + t.rows.len() - 4;
    assert!(below > 2, "the merge's rows are pasted: {:?}", merges(t));
    assert!(chains_hold(t), "{:?}", merges(t));
    assert_eq!(merges(t)[1][0], VMerge::Restart, "{:?}", merges(t));
    assert_eq!(merges(t)[below][0], VMerge::Restart, "the merged cell's part below starts again: {:?}", merges(t));
}

#[test]
fn merge_over_a_gap_row_leaves_unselected_cells_alone() {
    let rows = [
        tr("<w:gridAfter w:val=\"2\"/>", &[tc("A", ""), tc("B", "")]),
        tr("", &[tc("c0", ""), tc("c1", ""), tc("c2", ""), tc("c3", "")]),
        tr("", &[tc("d0", ""), tc("d1", ""), tc("d2", ""), tc("d3", "")]),
    ]
    .concat();
    let mut e = editor(4, &rows);
    cells(&mut e, CellShape::Columns, (0, 2), (2, 2));
    let out = e.execute(Command::MergeCells, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = table(&e);
    assert_eq!((cell_text(t, 0, 0), cell_text(t, 0, 1)), ("A".into(), "B".into()), "unselected cell text must not change");
    assert_eq!([t.rows[1].cells[2].v_merge, t.rows[2].cells[2].v_merge], [VMerge::Restart, VMerge::Continue], "the cells below the gap merge");
    assert_eq!(texts(&e).iter().filter(|t| *t == "c2" || *t == "d2").count(), 2, "with both their texts");
    assert_eq!(violations(&e), Vec::<String>::new());
}

#[test]
fn split_of_a_vertically_merged_cell_keeps_the_chain_consistent() {
    let rows = [
        tr("", &[tc("a", "<w:vMerge w:val=\"restart\"/>"), tc("b", "")]),
        tr("", &[tc("", "<w:vMerge/>"), tc("c", "")]),
        tr("", &[tc("", "<w:vMerge/>"), tc("d", "")]),
    ]
    .concat();
    let mut e = editor(2, &rows);
    caret_in(&mut e, 0, 0);
    let out = e.execute(Command::SplitCells(SplitCells { columns: 2, rows: None }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = table(&e);
    let shape: Vec<_> = t.rows.iter().map(|r| r.cells.iter().map(|c| (c.span, c.v_merge)).collect::<Vec<_>>()).collect();
    assert!(t.rows.iter().all(|r| r.cells.len() == 3), "every row of the chain splits: {shape:?}");
    assert_eq!(t.rows[0].cells[1].v_merge, VMerge::Restart);
    assert_eq!(t.rows[1].cells[1].v_merge, VMerge::Continue);
    assert!(chains_hold(t));
}

#[test]
fn delete_cells_left_does_not_attach_a_continue_to_an_unrelated_cell() {
    let rows =
        [tr("", &[tc("A", ""), tc("B", ""), tc("C", "<w:vMerge w:val=\"restart\"/>")]), tr("", &[tc("a", ""), tc("b", ""), tc("", "<w:vMerge/>")])]
            .concat();
    let mut e = editor(3, &rows);
    caret_in(&mut e, 1, 0);
    e.execute(Command::DeleteCells(CellShift::Left), fonts());
    let t = table(&e);
    assert!(chains_hold(t));
    for (k, c) in t.rows[1].cells.iter().enumerate() {
        if c.v_merge == VMerge::Continue {
            assert_ne!(t.rows[0].cells[k].v_merge, VMerge::None, "Continue at {k} sits under an unrelated cell");
        }
    }
}

#[test]
fn delete_cells_left_does_not_move_a_continue_onto_another_merge() {
    let (r, c) = ("<w:vMerge w:val=\"restart\"/>", "<w:vMerge/>");
    let rows =
        [tr("", &[tc("X", ""), tc("B", r), tc("C", r)]), tr("", &[tc("A", ""), tc("", c), tc("", c)]), tr("", &[tc("x", ""), tc("", c), tc("", c)])]
            .concat();
    let mut e = editor(3, &rows);
    caret_in(&mut e, 1, 0);
    e.execute(Command::DeleteCells(CellShift::Left), fonts());
    let t = table(&e);
    assert!(chains_hold(t), "{:?}", merges(t));
    assert!(t.rows[1].cells.iter().all(|c| c.v_merge != VMerge::Continue), "{:?}", merges(t));
    assert_eq!(merges(t)[2], [VMerge::None, VMerge::None, VMerge::None], "the rows below lose the shifted merges");
    assert_eq!(merges(t)[0], [VMerge::None, VMerge::None, VMerge::None]);
}

#[test]
fn delete_cells_left_keeps_a_merge_that_moves_whole() {
    let (r, c) = ("<w:vMerge w:val=\"restart\"/>", "<w:vMerge/>");
    let rows = [tr("", &[tc("A", ""), tc("B", r)]), tr("", &[tc("a", ""), tc("", c)]), tr("", &[tc("x", ""), tc("y", "")])].concat();
    let mut e = editor(2, &rows);
    cells(&mut e, CellShape::Rect, (0, 0), (1, 0));
    e.execute(Command::DeleteCells(CellShift::Left), fonts());
    let t = table(&e);
    assert_eq!(merges(t)[..2], [vec![VMerge::Restart], vec![VMerge::Continue]]);
    assert!(chains_hold(t));
}

#[test]
fn insert_table_uses_the_width_of_the_section_it_is_in() {
    let wide = "<w:p><w:pPr><w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"3600\" w:bottom=\"1440\" w:left=\"3600\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/></w:sectPr></w:pPr><w:r><w:t>one</w:t></w:r></w:p>";
    let mut e = open_xml(&format!("{wide}<w:p><w:r><w:t>two</w:t></w:r></w:p>"));
    let s = &e.document().sections;
    assert_eq!(s.len(), 2);
    let want = s[1].page_w - s[1].left - s[1].right;
    assert!((s[0].page_w - s[0].left - s[0].right - want).abs() > 1.0, "sections differ");
    let last = crate::layout::ParaId(match s[1].blocks.last() {
        Some(Block::Para(p)) => p.id,
        _ => unreachable!(),
    });
    e.select_between((last, Unit(0)), (last, Unit(0)), fonts());
    let out = e.execute(Command::InsertTable(crate::api::command::TableSize { rows: 1, columns: 2 }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = e.document().sections[1].blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).expect("table in section 2");
    let sum: f64 = t.grid.iter().sum();
    assert!((sum - want).abs() < 1.0, "grid {sum} vs section text width {want}");
}

#[test]
fn insert_table_in_a_two_column_section_fits_the_column() {
    let cols = "<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/><w:cols w:num=\"2\" w:space=\"720\"/></w:sectPr>";
    let mut e = open_xml(&format!("<w:p><w:r><w:t>x</w:t></w:r></w:p>{cols}"));
    caret(&mut e, 0);
    e.execute(cmd(serde_json::json!({"type":"insertTable","rows":1,"columns":2})), fonts());
    let sum: f64 = table(&e).grid.iter().sum();
    assert!((sum - (468.0 - 36.0) / 2.0).abs() < 1.0, "grid {sum}");
}

#[test]
fn a_table_nested_in_a_narrow_cell_has_positive_widths() {
    let narrow = "<w:tc><w:tcPr><w:tcW w:w=\"100\" w:type=\"dxa\"/><w:tcMar><w:left w:w=\"108\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/></w:tcMar></w:tcPr><w:p/></w:tc>";
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr>{narrow}</w:tr></w:tbl>{END}"
    );
    let mut e = open_xml(&body);
    caret(&mut e, 0);
    let out = e.execute(cmd(serde_json::json!({"type":"insertTable","rows":1,"columns":2})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let Some(Block::Table(inner)) = table(&e).rows[0].cells[0].blocks.iter().find(|b| matches!(b, Block::Table(_))) else { panic!("nested table") };
    assert!(inner.grid.iter().all(|w| *w > 0.0), "{:?}", inner.grid);
}

#[test]
fn delete_columns_updates_the_width_of_a_shrunk_merged_cell() {
    let rows = [
        tr(
            "",
            &[
                tc("A", ""),
                "<w:tc><w:tcPr><w:tcW w:w=\"2880\" w:type=\"dxa\"/><w:gridSpan w:val=\"2\"/></w:tcPr><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>"
                    .to_string(),
            ],
        ),
        tr("", &[tc("a", ""), tc("b", ""), tc("c", "")]),
    ]
    .concat();
    let mut e = editor(3, &rows);
    cells(&mut e, CellShape::Rect, (1, 2), (1, 2));
    let out = e.execute(Command::DeleteColumns, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = table(&e);
    assert_eq!(t.rows[0].cells[1].span, 1);
    assert_eq!(t.rows[0].cells[1].width, Some(crate::docx::props::Width::Dxa(t.grid[1])));
}

#[test]
fn split_into_columns_and_rows_makes_a_grid() {
    let mut e = editor(1, &tr("", &[tc("x", "")]));
    caret_in(&mut e, 0, 0);
    let out = e.execute(Command::SplitCells(SplitCells { columns: 2, rows: Some(2) }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = table(&e);
    assert_eq!(t.rows.len(), 2);
    assert!(t.rows.iter().all(|r| r.cells.len() == 2 && r.cells.iter().all(|c| c.v_merge == VMerge::None)));
}

#[test]
fn split_cells_splits_every_selected_cell() {
    let mut e = editor(2, &[tr("", &[tc("a", ""), tc("b", "")]), tr("", &[tc("c", ""), tc("d", "")])].concat());
    cells(&mut e, CellShape::Rect, (0, 0), (0, 1));
    let out = e.execute(Command::SplitCells(SplitCells { columns: 2, rows: None }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(spans(&e), vec![vec![1, 1, 1, 1], vec![2, 2]]);
    assert_eq!(cell_text(table(&e), 0, 2), "b");
}

#[test]
fn move_rows_refuses_to_split_a_repeating_section() {
    let sdt = format!(
        "<w:sdt><w:sdtPr><w:id w:val=\"7\"/></w:sdtPr><w:sdtContent>{}{}</w:sdtContent></w:sdt>",
        tr("", &[tc("s1", "")]),
        tr("", &[tc("s2", "")])
    );
    let rows = format!("{}{sdt}", tr("", &[tc("o", "")]));
    let mut e = editor(1, &rows);
    assert!(!table(&e).rows[1].controls.is_empty(), "row 1 is in the control");
    caret_in(&mut e, 1, 0);
    let out = e.execute(Command::MoveRows(VerticalDirection::Up), fonts());
    assert!(matches!(&out, Outcome::Refused(e) if e.message == "region boundary"), "{out:?}");
    assert_eq!(cell_text(table(&e), 0, 0), "o");
}

#[test]
fn autofit_fixed_pins_the_drawn_column_widths() {
    let mut e = editor(2, &tr("", &[tc("a", ""), tc("b", "")]));
    assert!(!table(&e).fixed);
    caret_in(&mut e, 0, 0);
    let out = e.execute(cmd(serde_json::json!({"type":"autoFitColumns","value":"fixed"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = table(&e);
    assert!(t.fixed);
    assert_eq!(t.grid.len(), 2, "{:?}", t.grid);
    assert!(t.rows[0].cells.iter().zip(&t.grid).all(|(c, w)| c.width == Some(crate::docx::props::Width::Dxa(*w))));
    let back = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert!(table(&back).fixed, "saved as tblLayout fixed");
    e.execute(Command::Undo, fonts());
    assert!(!table(&e).fixed, "undo restores the layout");
}

#[test]
fn autofit_window_spans_the_text_width() {
    let mut e = editor(2, &tr("", &[tc("a", ""), tc("b", "")]));
    caret_in(&mut e, 0, 0);
    let out = e.execute(cmd(serde_json::json!({"type":"autoFitColumns","value":"window"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = table(&e);
    assert_eq!(t.width, crate::docx::props::Width::Pct(1.0));
    let s = &e.document().sections[0];
    let sum: f64 = t.grid.iter().sum();
    assert!((sum - (s.page_w - s.left - s.right)).abs() < 1.0, "grid {:?}", t.grid);
    assert!((t.grid[0] - t.grid[1]).abs() < 0.1, "the columns keep their proportions");
}

#[test]
fn distribute_rows_in_a_header_table_measures_each_row_once() {
    let page = "<w:p><w:r><w:t>body</w:t></w:r><w:r><w:br w:type=\"page\"/></w:r></w:p>";
    let mut e = open_xml(&format!("{page}{page}<w:p><w:r><w:t>last</w:t></w:r></w:p>"));
    e.execute(Command::EditHeader, fonts());
    e.execute(cmd(serde_json::json!({"type":"insertTable","rows":2,"columns":1})), fonts());
    e.execute(cmd(serde_json::json!({"type":"insertText","value":"one"})), fonts());
    e.execute(cmd(serde_json::json!({"type":"moveToCell","value":"next"})), fonts());
    for _ in 0..3 {
        e.execute(cmd(serde_json::json!({"type":"insertText","value":"line"})), fonts());
        e.execute(Command::InsertParagraph, fonts());
    }
    let id = e.index().table_ids().next().expect("the header table");
    e.complete(fonts());
    let geos = e.page_geometries();
    assert!(geos.len() >= 3, "three pages");
    let heights: Vec<f64> = geos[0].1.rows.iter().filter(|r| r.table == id && !r.repeat).map(|r| r.rect[3] - r.rect[1]).collect();
    assert_eq!(heights.len(), 2);
    let out = e.execute(Command::DistributeRows, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let t = e.index().table(e.document(), id).expect("the header table");
    let want = (heights[0] + heights[1]) / 2.0;
    for r in &t.rows {
        let (h, rule) = r.height.clone().expect("a height");
        assert!((h - want).abs() < 0.5 && rule == "atLeast", "{h} vs {want}");
    }
}

#[test]
fn setting_one_cell_edge_keeps_the_cells_other_borders_as_written() {
    let own = "<w:tcBorders><w:start w:val=\"double\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideH w:val=\"single\" w:sz=\"8\" w:space=\"0\" w:color=\"auto\"/></w:tcBorders>";
    let mut e = editor(2, &tr("", &[tc("a", own), tc("b", "")]));
    caret_in(&mut e, 0, 0);
    let out = e.execute(cmd(serde_json::json!({"type":"setCellBorders","edges":"top"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let raw = table(&e).rows[0].cells[0].direct.0.as_deref().unwrap_or_default().to_string();
    assert!(raw.contains("<w:top "), "{raw}");
    assert!(raw.contains("<w:start ") && raw.contains("<w:insideH "), "the cell's own borders stay: {raw}");
    assert!(!raw.contains("<w:left ") && !raw.contains("<w:bottom ") && !raw.contains("<w:right "), "only the edge set is written: {raw}");
    let back = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(texts(&back), texts(&e));
    assert!(table(&back).rows[0].cells[0].left.as_ref().is_some_and(|b| b.style == "double"));
}

fn body_shape(e: &Editor) -> Vec<String> {
    e.document().sections[0]
        .blocks
        .iter()
        .map(|b| match b {
            Block::Para(p) => super::UnitText::of(p).text,
            Block::Table(t) => format!("table {}x{}", t.rows.len(), t.grid.len()),
        })
        .collect()
}

fn select_paras(e: &mut Editor, a: usize, b: usize) {
    let (pa, pb) = (para_id(e, a), para_id(e, b));
    let end = crate::editing::step::para_len(e.index().para(e.document(), pb).expect("a paragraph"));
    e.select_between((pa, Unit(0)), (pb, end), fonts());
}

const TABBED: &str =
    "<w:p><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t></w:r></w:p><w:p><w:r><w:t>c</w:t><w:tab/><w:t>d</w:t></w:r></w:p><w:p><w:r><w:t>e</w:t></w:r></w:p>";

#[test]
fn text_converts_to_a_table_a_row_a_paragraph() {
    let mut e = open_xml(&format!("{TABBED}{END}"));
    let before = texts(&e);
    select_paras(&mut e, 0, 2);
    let out = e.execute(cmd(serde_json::json!({"type":"convertTextToTable","separator":"tabs"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(body_shape(&e), ["table 3x2", "end"]);
    let t = table(&e);
    let got: Vec<String> = (0..3).flat_map(|r| (0..2).map(move |c| (r, c))).map(|(r, c)| cell_text(t, r, c)).collect();
    assert_eq!(got, ["a", "b", "c", "d", "e", ""]);
    assert_eq!(violations(&e), Vec::<String>::new());
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), before, "one undo step restores the paragraphs");
}

#[test]
fn text_with_more_pieces_than_columns_goes_on_over_the_next_row() {
    let mut e = open_xml(&format!("<w:p><w:r><w:t>1,2,3</w:t></w:r></w:p>{END}"));
    select_paras(&mut e, 0, 0);
    e.execute(cmd(serde_json::json!({"type":"convertTextToTable","separator":"commas","columns":2})), fonts());
    let t = table(&e);
    assert_eq!(spans(&e), vec![vec![1, 1]; 2]);
    assert_eq!([cell_text(t, 0, 0), cell_text(t, 0, 1), cell_text(t, 1, 0), cell_text(t, 1, 1)], ["1", "2", "3", ""]);
}

#[test]
fn a_numbered_paragraph_converts_to_one_numbered_cell() {
    let mut e = super::open("FX-LISTS.docx");
    let num = |p: &crate::docx::Paragraph| p.style.num.is_some_and(|(n, _)| n != 0);
    let at = e.document().sections[0].blocks.iter().position(|b| matches!(b, Block::Para(p) if num(p))).expect("a numbered paragraph");
    let Block::Para(p) = &e.document().sections[0].blocks[at] else { unreachable!() };
    let (id, len) = (crate::layout::ParaId(p.id), crate::editing::step::para_len(p));
    e.select_between((id, len), (id, len), fonts());
    e.execute(Command::InsertTab, fonts());
    e.execute(Command::InsertText("b".into()), fonts());
    let end = crate::editing::step::para_len(e.index().para(e.document(), id).expect("the paragraph"));
    e.select_between((id, Unit(0)), (id, end), fonts());
    let num = |e: &Editor| {
        (0..2)
            .map(|c| match &table(e).rows[0].cells[c].blocks[0] {
                Block::Para(p) => num(p),
                Block::Table(_) => false,
            })
            .collect::<Vec<_>>()
    };
    let out = e.execute(cmd(serde_json::json!({"type":"convertTextToTable","separator":"tabs"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(cell_text(table(&e), 0, 1), "b");
    assert_eq!(num(&e), [true, false], "the first piece keeps the numbering");
    let back = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(num(&back), [true, false], "saved so");
}

#[test]
fn text_beside_a_table_converts_to_a_separate_table() {
    let mut e = open_xml(&format!("{}<w:p><w:r><w:t>x;y</w:t></w:r></w:p>", tbl(1, &[row(&[cell(1, "", "a")])])));
    select_paras(&mut e, 1, 1);
    e.execute(cmd(serde_json::json!({"type":"convertTextToTable","separator":"other","character":";"})), fonts());
    assert_eq!(body_shape(&e), ["table 1x1", "", "table 1x2", ""]);
    let back = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(body_shape(&back), body_shape(&e));
}

#[test]
fn a_table_converts_to_text_a_paragraph_a_row() {
    let mut e = three_by_two();
    caret(&mut e, 0);
    let out = e.execute(cmd(serde_json::json!({"type":"convertTableToText","separator":"tabs"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(body_shape(&e), ["a\tb\tc", "d\te\tf", "end"]);
    e.execute(Command::Undo, fonts());
    assert_eq!(body_shape(&e), ["table 2x3", "end"]);
}

#[test]
fn selected_rows_convert_to_text_between_the_rest() {
    let r = |a: &str, b: &str| row(&[cell(1, "", a), cell(1, "", b)]);
    let mut e = open_xml(&format!("{}{END}", tbl(2, &[r("a", "b"), r("c", "d"), r("e", "f")])));
    cells(&mut e, CellShape::Rows, (1, 0), (1, 1));
    e.execute(cmd(serde_json::json!({"type":"convertTableToText","separator":"paragraphs"})), fonts());
    assert_eq!(body_shape(&e), ["table 1x2", "c", "d", "table 1x2", "end"]);
    assert_eq!(violations(&e), Vec::<String>::new());
    let back = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(body_shape(&back), body_shape(&e));
}

#[test]
fn a_table_in_a_content_control_converts_to_text_inside_it() {
    let sdt = |id: u32, inner: &str| format!("<w:sdt><w:sdtPr><w:id w:val=\"{id}\"/></w:sdtPr><w:sdtContent>{inner}</w:sdtContent></w:sdt>");
    let nested = tbl(1, &[row(&[cell(1, "", "in")])]);
    let deep = format!("<w:tc><w:tcPr><w:tcW w:w=\"1440\" w:type=\"dxa\"/></w:tcPr>{nested}<w:p/></w:tc>");
    let rows = [row(&[cell(1, "", "a")]), row(std::slice::from_ref(&deep)), row(&[cell(1, "", "b")])];
    let in_table = format!("{}{END}", sdt(1, &tbl(1, &rows)));
    let by_row = format!("{}{END}", tbl(1, &[rows[0].clone(), sdt(2, &rows[1]), rows[2].clone()]));
    // How many controls each body block is in after the conversion
    let wants: [[&[usize]; 2]; 2] = [[&[1, 1, 1, 1, 0], &[1, 1, 1, 1, 1, 0]], [&[0, 1, 1, 0, 0], &[0, 0, 1, 1, 0, 0]]];
    for (body, wants) in [&in_table, &by_row].into_iter().zip(wants) {
        for (picked, want) in [None, Some(1)].into_iter().zip(wants) {
            let mut e = open_xml(body);
            match picked {
                Some(r) => cells(&mut e, CellShape::Rows, (r, 0), (r, 0)),
                None => caret(&mut e, 0),
            }
            let out = e.execute(cmd(serde_json::json!({"type":"convertTableToText","separator":"paragraphs"})), fonts());
            assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
            assert_eq!(violations(&e), Vec::<String>::new(), "{picked:?} of {body}");
            let back = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
            assert_eq!(body_shape(&back), body_shape(&e));
            let live = controlled(&e);
            assert_eq!(live.iter().map(|b| b.1).collect::<Vec<_>>(), want, "{picked:?} of {body}: {live:?}");
            assert_eq!(controlled(&back), live, "{picked:?} of {body}: reopened in the same controls");
        }
    }
}

#[test]
fn a_tracked_table_to_text_rejects_back_to_the_table() {
    let mut e = three_by_two();
    e.set_tracking(true);
    caret(&mut e, 0);
    let out = e.execute(cmd(serde_json::json!({"type":"convertTableToText","separator":"commas"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(!super::super::revisions::index::revisions(e.document(), e.index()).is_empty(), "the conversion is tracked");
    e.resolve_revisions(false, &Scope::All, fonts());
    assert_eq!(body_shape(&e), ["table 2x3", "end"]);
}

#[test]
fn accepting_a_tracked_column_deletion_narrows_a_merged_cell_over_it() {
    let rows = [tr("", &[tc("A", ""), cell(2, "", "B")]), tr("", &[tc("a", ""), tc("b", ""), tc("c", "")])].concat();
    let mut e = editor(3, &rows);
    e.set_tracking(true);
    cells(&mut e, CellShape::Rect, (1, 2), (1, 2));
    e.execute(Command::DeleteColumns, fonts());
    assert_eq!(spans(&e), vec![vec![1, 2], vec![1, 1, 1]], "the merged cell keeps its span until accepted");
    e.resolve_revisions(true, &Scope::All, fonts());
    assert_eq!(spans(&e), vec![vec![1, 1], vec![1, 1]]);
    let t = table(&e);
    assert_eq!(t.grid.len(), 2);
    assert_eq!(t.rows[0].cells[1].width, Some(crate::docx::props::Width::Dxa(t.grid[1])));
}

/// Each body block's shape and how many content controls it is in.
fn controlled(e: &Editor) -> Vec<(String, usize)> {
    let controls = |b: &Block| match b {
        Block::Para(p) => p.controls.len(),
        Block::Table(t) => t.controls.len(),
    };
    body_shape(e).into_iter().zip(e.document().sections[0].blocks.iter().map(controls)).collect()
}

fn sdt(id: u32, inner: &str) -> String {
    format!("<w:sdt><w:sdtPr><w:id w:val=\"{id}\"/></w:sdtPr><w:sdtContent>{inner}</w:sdtContent></w:sdt>")
}

/// A merge chain down column 0 of three rows.
const CHAIN: [(&str, &str); 3] = [("a", "<w:vMerge w:val=\"restart\"/>"), ("", "<w:vMerge/>"), ("", "<w:vMerge/>")];

#[test]
fn merging_part_of_a_vertical_merge_is_refused() {
    let rows: String = CHAIN.iter().zip(["b", "c", "d"]).map(|(&(t, m), b)| tr("", &[tc(t, m), tc(b, "")])).collect();
    let mut e = editor(2, &rows);
    // Text from cell to cell: a cell selection takes in merged cells whole.
    let para = |e: &Editor, (r, c): (usize, usize)| match &table(e).rows[r].cells[c].blocks[0] {
        Block::Para(p) => crate::layout::ParaId(p.id),
        _ => unreachable!(),
    };
    for (a, f) in [((0, 0), (1, 1)), ((1, 0), (2, 1))] {
        let (pa, pf) = (para(&e, a), para(&e, f));
        e.select_between((pa, Unit(0)), (pf, Unit(1)), fonts());
        let out = e.execute(Command::MergeCells, fonts());
        assert!(matches!(out, Outcome::Unchanged(_)), "{a:?} to {f:?}: {out:?}");
        assert_eq!(spans(&e), vec![vec![1, 1]; 3]);
    }
    cells(&mut e, CellShape::Rect, (0, 0), (2, 1));
    let out = e.execute(Command::MergeCells, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "the whole merged cell merges: {out:?}");
    assert!(chains_hold(table(&e)), "{:?}", merges(table(&e)));
    assert_eq!(violations(&e), Vec::<String>::new());
}

#[test]
fn tidied_merges_end_where_the_cells_span_other_columns() {
    let e = editor(2, &[tr("", &[cell(2, "<w:vMerge w:val=\"restart\"/>", "a")]), tr("", &[tc("", "<w:vMerge/>"), tc("b", "")])].concat());
    let mut rows = table(&e).rows.clone();
    super::super::tables::tidy_merges(&mut rows);
    assert!(
        rows.iter().flat_map(|r| &r.cells).all(|c| c.v_merge == VMerge::None),
        "{:?}",
        rows.iter().map(|r| r.cells.iter().map(|c| c.v_merge).collect::<Vec<_>>()).collect::<Vec<_>>()
    );
}

#[test]
fn merging_cells_keeps_each_cell_content_control_whole() {
    let mut e = editor(3, &row(&[sdt(1, &tc("a", "")), sdt(2, &tc("b", "")), tc("c", "")]));
    assert!(!table(&e).rows[0].cells[0].controls.is_empty(), "setup: cell-level controls");
    for (a, f) in [(0, 1), (1, 2)] {
        cells(&mut e, CellShape::Rect, (0, a), (0, f));
        let out = e.execute(Command::MergeCells, fonts());
        assert!(matches!(&out, Outcome::Refused(e) if e.message == "region boundary"), "{a}-{f}: {out:?}");
        assert_eq!(spans(&e), vec![vec![1, 1, 1]]);
    }
    // Cells in the same control merge inside it.
    let mut e = editor(3, &row(&[sdt(1, &[tc("a", ""), tc("b", "")].concat()), tc("c", "")]));
    cells(&mut e, CellShape::Rect, (0, 0), (0, 1));
    let out = e.execute(Command::MergeCells, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(spans(&e), vec![vec![2, 1]]);
    assert!(!table(&e).rows[0].cells[0].controls.is_empty() && table(&e).rows[0].cells[1].controls.is_empty());
    assert_eq!(violations(&e), Vec::<String>::new());
}

#[test]
fn region_checks_take_linear_time_in_the_rows() {
    let time = |n: usize| {
        let rows: String = (0..n / 2).map(|_| tr("", &[tc("x", "")])).collect();
        let doc = crate::docx::read_editable(&blank_body(&format!("{}{END}", tbl(1, &[sdt(1, &rows), rows.clone()])))).expect("reads");
        let Some(Block::Table(t)) = doc.sections[0].blocks.first() else { panic!("a table") };
        assert_eq!(t.rows.iter().filter(|r| r.controls.len() == 1).count(), n / 2, "setup: half the rows in the control");
        (0..3)
            .map(|_| {
                let at = crate::clock::Instant::now();
                assert!(!super::super::tables::cuts_region(t, 0, n / 2 - 1, true));
                assert!(super::super::tables::cuts_region(t, 0, n - 1, true));
                at.elapsed().as_secs_f64()
            })
            .fold(f64::MAX, f64::min)
    };
    let (a, b) = (time(4000), time(16000));
    assert!(b < a * 8.0 + 0.01, "4x the rows took {:.2}x as long ({a:.4}s -> {b:.4}s)", b / a);
}

#[test]
fn a_table_on_a_sub_template_fits_the_section_it_was_entered_from() {
    use super::super::sub_templates::column_width;
    let mut e = super::open("FX-SUBTEMPLATES.docx");
    e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    e.complete(fonts());
    let s = &e.document().sections;
    let (want, first) = (column_width(&s[1]), column_width(&s[0]));
    assert!((want - first).abs() > 1.0, "setup: the sections differ");
    let Some(Block::Para(p)) = s[1].blocks.first() else { panic!("the landscape section's reference") };
    let at = crate::layout::ParaId(p.id);
    let field = e.references().by_field.iter().find(|(_, r)| r.at == at).map(|(f, _)| *f).expect("a reference");
    assert!(matches!(e.enter_sub_template(0, Some(field)), Outcome::Moved));
    let out = e.execute(Command::InsertTable(crate::api::command::TableSize { rows: 1, columns: 2 }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let body = &e.document().glossary.as_ref().expect("a glossary").parts[0].body;
    let t = body.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).expect("the table");
    let sum: f64 = t.grid.iter().sum();
    assert!((sum - want).abs() < 1.0, "grid {sum} vs the landscape column {want}");
}

/// `body` with a table style `Spaced` giving 6pt of cell spacing.
fn spaced_style(body: &str) -> Editor {
    let package = blank_body(body);
    let styles = crate::docx::package::read(&package, &[], "word/styles.xml").expect("reads").expect("styles");
    let style = "<w:style w:type=\"table\" w:styleId=\"Spaced\"><w:name w:val=\"Spaced\"/><w:tblPr><w:tblCellSpacing w:w=\"120\" w:type=\"dxa\"/></w:tblPr></w:style>";
    let styles = styles.replacen("</w:styles>", &format!("{style}</w:styles>"), 1);
    let package = crate::docx::write::repackage(&package, &[("word/styles.xml".into(), styles.into_bytes())], zip::CompressionMethod::Deflated)
        .expect("repackages");
    Editor::open(&package, fonts()).expect("opens")
}

#[test]
fn zero_cell_spacing_holds_over_a_styles_spacing() {
    let mut e = spaced_style(&format!(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"Spaced\"/><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1440\"/></w:tblGrid>{}</w:tbl>{END}",
        tr("", &[tc("a", "")])
    ));
    assert_eq!(table(&e).cell_spacing, 6.0, "setup: the style's spacing");
    caret_in(&mut e, 0, 0);
    let out = e.execute(cmd(serde_json::json!({"type":"setTableCellSpacing","value":0})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let out = e.execute(cmd(serde_json::json!({"type":"setTableStyleOptions","totalRow":true})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(table(&e).cell_spacing, 0.0, "resolving the style again keeps the table's own");
    let back = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(table(&back).cell_spacing, 0.0, "saved as the table's own");
    let out = e.execute(cmd(serde_json::json!({"type":"setTableCellSpacing","value":0})), fonts());
    assert!(matches!(out, Outcome::Unchanged(_)), "{out:?}");
}

#[test]
fn autofit_layout_and_width_hold_through_a_style_change() {
    for (fit, fixed, width) in [("fixed", true, crate::docx::props::Width::Auto), ("window", false, crate::docx::props::Width::Pct(1.0))] {
        let mut e = editor(2, &tr("", &[tc("a", ""), tc("b", "")]));
        caret_in(&mut e, 0, 0);
        let out = e.execute(cmd(serde_json::json!({"type":"autoFitColumns","value":fit})), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{fit}: {out:?}");
        let out = e.execute(cmd(serde_json::json!({"type":"setTableStyleOptions","totalRow":true})), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{fit}: {out:?}");
        assert_eq!((table(&e).fixed, table(&e).width), (fixed, width), "{fit}");
    }
}

#[test]
fn rows_converted_to_text_leave_no_merge_without_its_start() {
    let rows: String =
        CHAIN[..2].iter().zip(["b", "c"]).map(|(&(t, m), b)| tr("", &[tc(t, m), tc(b, "")])).chain([tr("", &[tc("x", ""), tc("y", "")])]).collect();
    let mut e = editor(2, &rows);
    // The first row alone, as a drag across its cells selects it
    let table_id = crate::layout::TableId(table(&e).id);
    e.set_caret_for_benchmark(Selection::Cells {
        table: table_id,
        anchor: CellRef { row: 0, col: 0 },
        focus: CellRef { row: 0, col: 1 },
        shape: CellShape::Rect,
    });
    let out = e.execute(cmd(serde_json::json!({"type":"convertTableToText","separator":"tabs"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(body_shape(&e), ["a\tb", "table 2x2", "end"]);
    assert_eq!(table(&e).rows[0].cells[0].v_merge, VMerge::None);
    assert_eq!(violations(&e), Vec::<String>::new());
}

#[test]
fn rejecting_a_width_change_and_an_inserted_column_restores_the_grid() {
    let mut e = editor(3, &tr("", &[tc("a", ""), tc("b", ""), tc("c", "")]));
    let was = table(&e).grid.clone();
    e.set_tracking(true);
    caret_in(&mut e, 0, 0);
    let out = e.execute(cmd(serde_json::json!({"type":"setColumnWidth","value":100})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(table(&e).grid_change.is_some(), "setup: a tracked grid change");
    caret_in(&mut e, 0, 0);
    let out = e.execute(cmd(serde_json::json!({"type":"insertColumns","where":"right"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(table(&e).rows[0].cells.len(), 4);
    e.resolve_revisions(false, &Scope::All, fonts());
    let t = table(&e);
    assert_eq!(t.grid, was);
    assert_eq!((0..3).map(|c| cell_text(t, 0, c)).collect::<Vec<_>>(), ["a", "b", "c"]);
}

#[test]
fn deleting_columns_with_ones_own_inserted_column_removes_it_outright() {
    let mut e = editor(2, &tr("", &[tc("a", ""), tc("b", "")]));
    e.set_tracking(true);
    caret_in(&mut e, 0, 1);
    let out = e.execute(cmd(serde_json::json!({"type":"insertColumns","where":"right"})), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(cell_marks(table(&e)), [[None, None, Some(crate::docx::review::CellMarkKind::Insert)]]);
    cells(&mut e, CellShape::Columns, (0, 1), (0, 2));
    let out = e.execute(Command::DeleteColumns, fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(cell_marks(table(&e)), [[None, Some(crate::docx::review::CellMarkKind::Delete)]], "the inserted cell goes, the other is marked");
    assert_eq!(table(&e).grid.len(), 2);
    assert_eq!(violations(&e), Vec::<String>::new());
    e.resolve_revisions(false, &Scope::All, fonts());
    assert_eq!((spans(&e), cell_marks(table(&e))), (vec![vec![1, 1]], vec![vec![None, None]]), "rejecting gives the table as it was");
    e.execute(Command::Undo, fonts());
    e.resolve_revisions(true, &Scope::All, fonts());
    assert_eq!(spans(&e), vec![vec![1]], "accepting deletes both");
}

fn text_to_table(sep: &str) -> crate::api::command::TextToTable {
    serde_json::from_value(serde_json::json!({ "separator": sep })).expect("options")
}

#[test]
fn text_to_table_sizes_its_columns_as_its_autofit_says() {
    use crate::docx::props::Width;
    use crate::editing::table_convert::ConvertFit;
    let wide = "<w:p><w:r><w:t>a</w:t><w:tab/><w:t>a much longer piece of text here</w:t></w:r></w:p>";
    let mut e = open_xml(&format!("{wide}{END}"));
    select_paras(&mut e, 0, 0);
    assert_eq!(e.text_to_table_size(&text_to_table("tabs")), Some((1, 2)));
    assert!(matches!(e.convert_text_to_table_fit(&text_to_table("tabs"), ConvertFit::Fixed(Some(90.0)), fonts()), Outcome::Changed(_)));
    assert_eq!((table(&e).grid.clone(), table(&e).width), (vec![90.0, 90.0], Width::Auto));
    e.execute(Command::Undo, fonts());
    select_paras(&mut e, 0, 0);
    assert!(matches!(e.convert_text_to_table_fit(&text_to_table("tabs"), ConvertFit::Window, fonts()), Outcome::Changed(_)));
    assert_eq!(table(&e).width, Width::Pct(1.0));
    e.execute(Command::Undo, fonts());
    select_paras(&mut e, 0, 0);
    assert!(matches!(e.convert_text_to_table_fit(&text_to_table("tabs"), ConvertFit::Contents, fonts()), Outcome::Changed(_)));
    let g = table(&e).grid.clone();
    assert!(g[0] < g[1], "fitted to contents: {g:?}");
    e.execute(Command::Undo, fonts());
    assert!(!body_shape(&e)[0].starts_with("table"), "one undo step");
    select_paras(&mut e, 0, 0);
    assert!(matches!(e.convert_text_to_table_fit(&text_to_table("tabs"), ConvertFit::Fixed(Some(0.0)), fonts()), Outcome::Refused(_)));
}

#[test]
fn text_to_table_previews_its_rows_and_columns() {
    let mut e = open_xml(&format!("{TABBED}{END}"));
    select_paras(&mut e, 0, 2);
    assert_eq!(e.text_to_table_size(&text_to_table("tabs")), Some((3, 2)));
    assert_eq!(e.text_to_table_size(&text_to_table("paragraphs")), Some((3, 1)));
    let two: crate::api::command::TextToTable = serde_json::from_value(serde_json::json!({"separator":"paragraphs","columns":2})).unwrap();
    assert_eq!(e.text_to_table_size(&two), Some((2, 2)));
}

#[test]
fn table_to_text_converts_nested_tables_when_asked() {
    let inner = "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"700\"/><w:gridCol w:w=\"700\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>";
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"1440\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc>{inner}</w:tc></w:tr></w:tbl>{END}"
    );
    let o: crate::api::command::TableToText = serde_json::from_value(serde_json::json!({"separator":"tabs"})).unwrap();
    let mut e = open_xml(&body);
    let first = para_id(&e, 0);
    e.set_selection(Selection::Caret(crate::editing::position::Position::text(crate::editing::position::StoryId::Body, first, Unit(0))));
    assert!(matches!(e.convert_table_to_text_nested(&o, false, fonts()), Outcome::Changed(_)));
    assert!(body_shape(&e).iter().any(|s| s.starts_with("table")), "kept: {:?}", body_shape(&e));
    e.execute(Command::Undo, fonts());
    e.set_selection(Selection::Caret(crate::editing::position::Position::text(crate::editing::position::StoryId::Body, first, Unit(0))));
    assert!(matches!(e.convert_table_to_text_nested(&o, true, fonts()), Outcome::Changed(_)));
    assert!(!body_shape(&e).iter().any(|s| s.starts_with("table")), "flattened: {:?}", body_shape(&e));
    assert!(texts(&e).contains(&"x\ty".to_string()), "{:?}", texts(&e));
}

/// With Track formatting off, a tracked row height, cell shading and vertical merge apply untracked; with it on,
/// each is a formatting revision.
#[test]
fn track_formatting_off_leaves_row_and_cell_changes_untracked() {
    use crate::api::value::{AutoOr, Color, HeightRule, Points, RowHeight};
    for formatting in [true, false] {
        let mut e = three_by_two();
        e.set_tracking(true);
        e.set_track_options(true, formatting);
        caret(&mut e, 0);
        e.execute(Command::SetRowHeight(AutoOr::Value(RowHeight { height: Points(30.0), rule: HeightRule::Exact })), fonts());
        e.execute(Command::SetCellShading(Some(Color(0xff0000))), fonts());
        // A tracked edit that merges cells vertically (Merge Cells itself is untracked, as in Word).
        let mut rows = table(&e).rows.clone();
        (rows[0].cells[2].v_merge, rows[1].cells[2].v_merge) = (VMerge::Restart, VMerge::Continue);
        let (id, grid) = (crate::layout::TableId(table(&e).id), table(&e).grid.clone());
        assert!(matches!(e.rebuild("Cell Properties", id, rows, grid, (0, 0), fonts()), Outcome::Changed(_)));
        let t = table(&e);
        assert!(t.rows[0].height.is_some() && t.rows[0].cells[0].shading.is_some(), "applied");
        assert_eq!([t.rows[0].cells[2].v_merge, t.rows[1].cells[2].v_merge], [VMerge::Restart, VMerge::Continue], "merged");
        let changes = (
            t.rows.iter().any(|r| r.rev.change.is_some()),
            t.rows.iter().flat_map(|r| &r.cells).any(|c| c.rev.change.is_some()),
            t.rows.iter().flat_map(|r| &r.cells).any(|c| matches!(c.rev.structural, Some((_, crate::docx::review::CellMarkKind::Merge { .. })))),
        );
        assert_eq!(changes, (formatting, formatting, formatting), "track formatting {formatting}");
    }
}

#[test]
fn table_to_text_keeps_a_nested_cells_content_control() {
    let inner = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"700\"/><w:gridCol w:w=\"700\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc><w:tc>{}</w:tc></w:tr></w:tbl><w:p/>",
        sdt(9, "<w:p><w:r><w:t>y</w:t></w:r></w:p>")
    );
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"1440\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc>{inner}</w:tc></w:tr></w:tbl>{END}"
    );
    let at = |e: &mut Editor| {
        let first = para_id(e, 0);
        e.set_selection(Selection::Caret(crate::editing::position::Position::text(crate::editing::position::StoryId::Body, first, Unit(0))));
    };
    let tabs: crate::api::command::TableToText = serde_json::from_value(serde_json::json!({"separator":"tabs"})).unwrap();
    let mut e = open_xml(&body);
    at(&mut e);
    assert!(!matches!(e.convert_table_to_text_nested(&tabs, true, fonts()), Outcome::Changed(_)), "y can't join x and keep its control");
    assert_eq!(body_shape(&e), ["table 1x2", "end"]);
    let paras: crate::api::command::TableToText = serde_json::from_value(serde_json::json!({"separator":"paragraphs"})).unwrap();
    at(&mut e);
    assert!(matches!(e.convert_table_to_text_nested(&paras, true, fonts()), Outcome::Changed(_)));
    let live = controlled(&e);
    assert_eq!(live.iter().map(|(t, n)| (t.as_str(), *n)).collect::<Vec<_>>(), [("a", 0), ("x", 0), ("y", 1), ("", 0), ("end", 0)], "{live:?}");
    assert_eq!(violations(&e), Vec::<String>::new());
}
