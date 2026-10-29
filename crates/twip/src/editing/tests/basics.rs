//! Typing, deletion, Enter, undo and navigation on real fixtures, natively.

use super::super::commands::{Command, Dir, MoveBy};
use super::super::position::{Place, StoryId};
use super::super::selection::Selection;
use super::{blank_with, fonts, open, texts};
use crate::layout::Unit;

fn type_text(e: &mut super::super::Editor, s: &str) {
    for c in s.chars() {
        e.execute(Command::InsertText(c.to_string()), fonts());
    }
}

#[test]
fn typing_inserts_at_the_caret_and_one_undo_removes_the_run() {
    let mut e = open("FX-TEXT.docx");
    let before = texts(&e);
    type_text(&mut e, "Hello");
    let after = texts(&e);
    assert!(after[0].starts_with("Hello"), "{:?}", after[0]);
    assert_eq!(after[0], format!("Hello{}", before[0]));
    // A continuous typing run is one undo step.
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), before);
    e.execute(Command::Redo, fonts());
    assert_eq!(texts(&e), after);
}

#[test]
fn enter_splits_and_backspace_joins_back() {
    let mut e = open("FX-TEXT.docx");
    let before = texts(&e);
    for _ in 0..3 {
        e.execute(crate::editing::tests::mv(MoveBy::Char, Dir::Forward, false), fonts());
    }
    e.execute(Command::InsertParagraph, fonts());
    let split = texts(&e);
    assert_eq!(split.len(), before.len() + 1);
    assert_eq!(format!("{}{}", split[0], split[1]), before[0]);
    e.execute(crate::editing::tests::del(Dir::Back, false), fonts());
    assert_eq!(texts(&e), before);
    // Undo the join, then the split.
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), split);
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), before);
}

#[test]
fn a_selection_is_replaced_by_typing() {
    let mut e = open("FX-TEXT.docx");
    let before = texts(&e);
    for _ in 0..5 {
        e.execute(crate::editing::tests::mv(MoveBy::Char, Dir::Forward, true), fonts());
    }
    type_text(&mut e, "X");
    let after = texts(&e);
    let rest: String = before[0].chars().skip(5).collect();
    assert_eq!(after[0], format!("X{rest}"));
}

#[test]
fn end_on_a_wrapped_line_shows_at_its_end_and_typing_goes_to_the_next_line_start() {
    // End sets end-of-line affinity; Right from there moves one character.
    let mut e = open("FX-TEXT.docx");
    // The third paragraph is long (the first is the heading).
    for _ in 0..2 {
        e.execute(crate::editing::tests::mv(MoveBy::Para, Dir::Forward, false), fonts());
    }
    e.execute(crate::editing::tests::mv(MoveBy::LineEdge, Dir::Forward, false), fonts());
    let Selection::Caret(p) = e.selection() else { panic!("a caret") };
    let Place::Text { offset, affinity, .. } = p.place else { panic!("text") };
    let first_len = texts(&e)[2].chars().count() as u32;
    assert!(offset.0 < first_len, "the first paragraph wraps");
    assert_eq!(affinity, super::super::position::Affinity::Upstream);
    let c = e.caret_view().expect("a caret rectangle");
    e.execute(crate::editing::tests::mv(MoveBy::Char, Dir::Forward, false), fonts());
    let Selection::Caret(q) = e.selection() else { panic!("a caret") };
    let Place::Text { offset: o2, .. } = q.place else { panic!("text") };
    assert_eq!(o2, Unit(offset.0 + 1));
    let c2 = e.caret_view().expect("a caret rectangle");
    assert!(c2.top > c.top, "after Right the caret is on the next line");
}

#[test]
fn down_then_up_returns_to_the_goal_x() {
    let mut e = open("FX-TEXT.docx");
    for _ in 0..12 {
        e.execute(crate::editing::tests::mv(MoveBy::Char, Dir::Forward, false), fonts());
    }
    let x0 = e.caret_view().expect("caret").x;
    e.execute(crate::editing::tests::mv(MoveBy::Line, Dir::Forward, false), fonts());
    e.execute(crate::editing::tests::mv(MoveBy::Line, Dir::Forward, false), fonts());
    e.execute(crate::editing::tests::mv(MoveBy::Line, Dir::Back, false), fonts());
    e.execute(crate::editing::tests::mv(MoveBy::Line, Dir::Back, false), fonts());
    let x1 = e.caret_view().expect("caret").x;
    assert!((x1 - x0).abs() < 6.0, "goal x kept: {x0} → {x1}");
}

#[test]
fn ctrl_end_and_ctrl_home_reach_the_story_ends() {
    let mut e = open("FX-TBL-BASIC.docx");
    e.execute(crate::editing::tests::mv(MoveBy::Story, Dir::Forward, false), fonts());
    let end = e.selection().focus().expect("caret");
    assert_eq!(end.story, StoryId::Body);
    e.execute(crate::editing::tests::mv(MoveBy::Story, Dir::Back, false), fonts());
    let start = e.selection().focus().expect("caret");
    assert!(e.index().cmp(&start, &end) == std::cmp::Ordering::Less);
}

#[test]
fn tab_in_the_last_cell_of_a_nested_table_adds_a_row_to_that_table_only() {
    let mut e = open("FX-NEST-2.docx");
    let fonts = fonts();
    // Find a nested table's last cell's paragraph and put the caret there.
    let doc = e.document();
    let mut target = None;
    fn find(blocks: &[crate::docx::Block], depth: usize, out: &mut Option<(u64, u64, usize)>) {
        for b in blocks {
            if let crate::docx::Block::Table(t) = b {
                if depth == 1 {
                    let last = t.rows.last().and_then(|r| r.cells.last()).and_then(|c| c.blocks.first());
                    if let Some(crate::docx::Block::Para(p)) = last {
                        *out = out.or(Some((t.id, p.id, t.rows.len())));
                    }
                }
                t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| find(&c.blocks, depth + 1, out));
            }
        }
    }
    find(&doc.sections[0].blocks, 0, &mut target);
    let (inner, para, rows) = target.expect("FX-NEST-2 has a nested table");
    let outer_rows: Vec<usize> = doc.sections[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            crate::docx::Block::Table(t) => Some(t.rows.len()),
            _ => None,
        })
        .collect();
    e.selection = Selection::Caret(super::super::position::Position::text(StoryId::Body, crate::layout::ParaId(para), Unit(0)));
    e.execute(Command::MoveToCell(crate::api::value::Order::Next), fonts);
    let t = e.index().table(e.document(), crate::layout::TableId(inner)).expect("inner table");
    assert_eq!(t.rows.len(), rows + 1, "the inner table gained a row");
    let outer_after: Vec<usize> = e.document().sections[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            crate::docx::Block::Table(t) => Some(t.rows.len()),
            _ => None,
        })
        .collect();
    assert_eq!(outer_after, outer_rows, "outer tables unchanged");
    let chain = e.index().chain_of(&e.selection().focus().expect("caret"));
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[1].row, rows, "the caret is in the new inner row");
    e.execute(Command::Undo, fonts);
    let t = e.index().table(e.document(), crate::layout::TableId(inner)).expect("inner table");
    assert_eq!(t.rows.len(), rows);
}

#[test]
fn every_click_on_a_line_lands_in_its_paragraph() {
    // Hit testing by containment: clicking the middle of every drawn line resolves to that line's paragraph,
    // at every nesting level.
    for name in ["FX-NEST-3.docx", "FX-TBL-MERGED.docx", "FX-TEXT.docx"] {
        let mut e = open(name);
        e.complete(fonts());
        let stack = e.stack();
        let n = stack.pages.len();
        for page in 0..n {
            let Some(geo) = e.pages.page(page, &e.index) else { continue };
            for l in &geo.lines {
                if geo.stories[l.story].story != StoryId::Body || l.stops.len() < 2 {
                    continue;
                }
                let x = (l.x0() + l.x1()) / 2.0;
                let y = (l.top + l.bottom) / 2.0;
                let hit = geo.hit_text(x, y, &|s| s == StoryId::Body).expect("a hit");
                assert_eq!(Some(hit.para), l.para(), "{name} page {page}: click at ({x:.1}, {y:.1})");
            }
        }
    }
}

#[test]
fn a_thousand_typed_characters_add_under_a_megabyte_of_undo_history() {
    // Memory, by the history's own accounting of what its steps hold.
    let mut e = open("FX-TEXT.docx");
    let before = e.history().bytes();
    type_text(&mut e, &"abcdefghij".repeat(100));
    assert!(e.history().bytes() - before <= 1 << 20, "{} bytes", e.history().bytes() - before);
    assert_eq!(e.history().undo_depth(), 1, "one typing run is one undo step");
}

#[test]
fn added_rows_and_typing_in_them_save_and_reopen() {
    // A row added by Tab and one by Enter at a row's end mark are written, with their text, and read back.
    let mut e = open("FX-TBL-BASIC.docx");
    let last = |e: &super::super::Editor| texts(e).iter().position(|t| t == "T2 R3 C3").expect("the second table's last cell");
    let p = last(&e);
    let para = super::super::tests::para_id(&e, p);
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(8))));
    e.execute(Command::MoveToCell(crate::api::value::Order::Next), fonts());
    type_text(&mut e, "new row");
    let edited = texts(&e);
    assert!(edited.contains(&"new row".to_string()));
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(texts(&again), edited);
}

#[test]
fn a_click_on_any_drawn_line_lands_in_that_lines_paragraph() {
    // Containment-first hit testing; every line of every (nested, merged) table is reachable.
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    for f in ["FX-NEST-3.docx", "FX-TBL-MERGED.docx", "FX-TBL-BASIC.docx", "FX-NEST-DEEP.docx", "FX-NEST-MIXED.docx", "FX-OBJ-CELL.docx"] {
        let mut e = open(f);
        e.complete(fonts());
        let stack = e.stack();
        let mut checked = 0;
        for (page, g) in e.page_geometries() {
            for l in g.lines.iter().filter(|l| !l.repeat && !l.chain.is_empty() && g.stories[l.story].story == StoryId::Body) {
                // Between the first two stops (or just right of a lone one): never on a cell's left-edge zone.
                let x = match l.stops.get(1) {
                    Some(s) => (l.stops[0].x + s.x) / 2.0,
                    None => l.stops[0].x + 1.0,
                };
                // A line pushed out of its own cell (an indent wider than the cell) is under another cell: containment wins there.
                let y = (l.top + l.bottom) / 2.0;
                let own = g.cells.iter().find(|c| c.chain == l.chain).expect("the line's cell");
                if x < own.rect[0] || x > own.rect[2] {
                    continue;
                }
                let (dx, dy) = stack.to_doc(page, x, y).expect("on the stack");
                for kind in [PointerKind::Down, PointerKind::Up] {
                    e.pointer_input(kind, dx, dy, 1, Mods::default(), fonts());
                }
                // A click on an inline picture selects it.
                let got = match e.selection() {
                    crate::editing::selection::Selection::Object(crate::editing::selection::ObjectRef::Inline { para, .. }) => Some(para),
                    s => s.focus().and_then(|p| p.para_offset()).map(|(p, _)| p),
                };
                let txt = |p: Option<crate::layout::ParaId>| {
                    p.and_then(|p| e.index().para(e.document(), p))
                        .map(|q| super::super::text::UnitText::of(q).text.chars().take(40).collect::<String>())
                };
                assert_eq!(
                    got,
                    Some(l.stops[0].para),
                    "{f}: a click at ({x:.1}, {:.1}) on a line of {:?} {:?} (page {page}, chain {:?}) landed in {:?}",
                    (l.top + l.bottom) / 2.0,
                    txt(Some(l.stops[0].para)),
                    l.stops[0].para,
                    l.chain,
                    txt(got)
                );
                checked += 1;
            }
        }
        assert!(checked > 5, "{f}: only {checked} table lines");
    }
}

#[test]
fn a_click_on_any_story_line_lands_in_that_line() {
    // Body, notes and text boxes take a click; headers and footers a double-click.
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    let mut e = open("FX-STORIES.docx");
    e.complete(fonts());
    let stack = e.stack();
    let mut seen = std::collections::HashSet::new();
    for (page, g) in e.page_geometries() {
        for l in g.lines.iter().filter(|l| l.stops.len() > 1) {
            let story = g.stories[l.story].story;
            if !seen.insert((story, l.stops[0].para)) {
                continue;
            }
            let x = (l.stops[0].x + l.stops[1].x) / 2.0;
            let (dx, dy) = stack.to_doc(page, x, (l.top + l.bottom) / 2.0).expect("on the stack");
            let clicks = if matches!(story, StoryId::HeaderFooter(_)) { 2 } else { 1 };
            for c in 1..=clicks {
                for kind in [PointerKind::Down, PointerKind::Up] {
                    e.pointer_input(kind, dx, dy, c, Mods::default(), fonts());
                }
            }
            let got = e.selection().focus().map(|p| (p.story, p.para_offset().map(|(p, _)| p)));
            assert_eq!(
                got,
                Some((story, Some(l.stops[0].para))),
                "a click on page {page} at ({x:.1}, {:.1}) on {story:?} {:?}; stories {:?}",
                (l.top + l.bottom) / 2.0,
                l.stops[0].para,
                g.stories.iter().map(|s| (s.story, s.rect.map(|v| v.round()))).collect::<Vec<_>>()
            );
            if let Some(cmd) = e.key_command("Escape", Mods::default()) {
                e.execute(cmd, fonts());
            }
        }
    }
    assert!(seen.len() > 20, "only {} story lines", seen.len());
}

#[test]
fn a_deleted_float_stays_deleted_after_saving() {
    // Delete with a float selected removes it; saving drops its drawing, the text kept.
    let count = |e: &super::super::Editor| {
        fn walk(b: &[crate::docx::Block]) -> usize {
            b.iter()
                .map(|b| match b {
                    crate::docx::Block::Para(p) => p.inlines.iter().filter(|i| matches!(i, crate::docx::Inline::Float(..))).count(),
                    crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).map(|c| walk(&c.blocks)).sum(),
                })
                .sum()
        }
        e.document().sections.iter().map(|s| walk(&s.blocks)).sum::<usize>()
    };
    let mut e = open("FX-OBJ.docx");
    let before = count(&e);
    let f = e
        .document()
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .find_map(|b| match b {
            crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                crate::docx::Inline::Float(f, _) => Some(crate::layout::FloatId(f.id)),
                _ => None,
            }),
            _ => None,
        })
        .expect("a float");
    e.set_caret_for_benchmark(Selection::Object(crate::editing::selection::ObjectRef::Floating(f)));
    e.execute(crate::editing::tests::del(super::super::commands::Dir::Forward, false), fonts());
    assert_eq!(count(&e), before - 1);
    let edited = texts(&e);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(count(&again), before - 1);
    assert_eq!(texts(&again), edited);
}

#[test]
fn edits_in_every_story_save_and_reopen() {
    // Typing in a header, a footnote, an endnote and a text box is written to their parts and read back.
    use crate::docx::{Block, Inline};
    fn first_para(b: &[Block]) -> Option<&crate::docx::Paragraph> {
        b.iter().find_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
    }
    fn text_box(doc: &crate::docx::Document) -> Option<(u64, &[Block])> {
        doc.sections.iter().flat_map(|s| s.blocks.iter()).find_map(|b| match b {
            Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                Inline::Float(f, _) => f.text_box.as_ref().map(|t| (f.id, t.blocks.as_slice())),
                _ => None,
            }),
            _ => None,
        })
    }
    let text = |p: &crate::docx::Paragraph| super::super::text::UnitText::of(p).text;
    let mut e = open("FX-STORIES.docx");
    let header = e.document().stories.keys().min().cloned().expect("a header");
    let targets: Vec<(StoryId, crate::layout::ParaId)> = {
        let d = e.document();
        let fid = *d.footnotes.keys().min().unwrap();
        let eid = *d.endnotes.keys().min().unwrap();
        let (tb, tbb) = text_box(d).expect("a text box");
        vec![
            (e.index().header_footer(&header).unwrap(), crate::layout::ParaId(first_para(&d.stories[&header]).unwrap().id)),
            (StoryId::Footnote(fid), crate::layout::ParaId(first_para(&d.footnotes[&fid]).unwrap().id)),
            (StoryId::Endnote(eid), crate::layout::ParaId(first_para(&d.endnotes[&eid]).unwrap().id)),
            (StoryId::TextBox(crate::layout::FloatId(tb)), crate::layout::ParaId(first_para(tbb).unwrap().id)),
        ]
    };
    for (story, para) in targets {
        e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(story, para, Unit(1))));
        type_text(&mut e, "Q");
    }
    let snap = |d: &crate::docx::Document| {
        let mut f: Vec<String> = d.footnotes.values().chain(d.endnotes.values()).filter_map(|b| first_para(b)).map(text).collect();
        f.sort();
        (text(first_para(&d.stories[&header]).unwrap()), f, text(first_para(text_box(d).unwrap().1).unwrap()))
    };
    let edited = snap(e.document());
    assert!(edited.0.contains('Q') && edited.2.contains('Q') && edited.1.iter().filter(|t| t.contains('Q')).count() == 2, "{edited:?}");
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(snap(again.document()), edited);
    assert_eq!(texts(&again), texts(&e));
}

#[test]
fn a_nudged_float_saves_its_new_offsets() {
    // A moved float's posOffset is patched in place and read back.
    let floats = |e: &super::super::Editor| -> Vec<(f64, f64)> {
        e.document()
            .sections
            .iter()
            .flat_map(|s| s.blocks.iter())
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
            .flat_map(|p| p.inlines.iter())
            .filter_map(|i| if let crate::docx::Inline::Float(f, _) = i { Some((f.h_offset, f.v_offset)) } else { None })
            .collect()
    };
    let mut e = open("FX-OBJ.docx");
    let id = e
        .document()
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let crate::docx::Inline::Float(f, _) = i { Some(crate::layout::FloatId(f.id)) } else { None })
        .nth(1)
        .expect("two floats");
    e.set_caret_for_benchmark(Selection::Object(crate::editing::selection::ObjectRef::Floating(id)));
    e.execute(crate::editing::tests::mv(super::super::commands::MoveBy::Char, super::super::commands::Dir::Forward, false), fonts());
    e.execute(crate::editing::tests::mv(super::super::commands::MoveBy::Line, super::super::commands::Dir::Forward, false), fonts());
    let moved = floats(&e);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    let back = floats(&again);
    assert_eq!(back.len(), moved.len());
    for (a, b) in back.iter().zip(&moved) {
        assert!((a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01, "{back:?} vs {moved:?}");
    }
    assert_eq!(texts(&again), texts(&e));
}

#[test]
fn a_resized_float_saves_its_new_size() {
    // A corner drag's new size (a locked picture: its ratio kept) is patched into the drawing's extent
    // and read back by the reader.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-OBJ-LOCK.docx", "resize-save");
    r.step(&json!({ "click": { "float": 1 } }));
    r.step(&json!({ "drag": { "from": { "float": 1, "at": "bottom-right" }, "to": { "float": 1, "at": "bottom-right", "dx": 50.4 } } }));
    let size = |e: &super::super::Editor| -> Vec<(f64, f64)> {
        e.document()
            .sections
            .iter()
            .flat_map(|s| s.blocks.iter())
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
            .flat_map(|p| p.inlines.iter())
            .filter_map(|i| if let crate::docx::Inline::Float(f, _) = i { Some((f.w, f.h)) } else { None })
            .collect()
    };
    let edited = size(&r.e);
    assert!((edited[1].0 - 151.2).abs() < 0.01 && (edited[1].1 - 108.0).abs() < 0.01, "{edited:?}");
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-OBJ.resize.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    for (a, b) in size(&again).iter().zip(&edited) {
        assert!((a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01, "{:?} vs {edited:?}", size(&again));
    }
}

#[test]
fn resized_columns_and_rows_save_and_reopen() {
    // A column border drag's grid and cell widths, and a row border drag's "at least" height, are
    // written and read back.
    use serde_json::json;
    let tables = |e: &super::super::Editor| -> Vec<(Vec<f64>, Vec<Option<(f64, String)>>, Vec<Option<crate::docx::props::Width>>)> {
        fn walk(b: &[crate::docx::Block], out: &mut Vec<(Vec<f64>, Vec<Option<(f64, String)>>, Vec<Option<crate::docx::props::Width>>)>) {
            for b in b {
                if let crate::docx::Block::Table(t) = b {
                    out.push((
                        t.grid.clone(),
                        t.rows.iter().map(|r| r.height.clone()).collect(),
                        t.rows.iter().flat_map(|r| r.cells.iter().map(|c| c.width)).collect(),
                    ));
                }
            }
        }
        let mut out = Vec::new();
        e.document().sections.iter().for_each(|s| walk(&s.blocks, &mut out));
        out
    };
    let mut r = super::scenarios::Run::open("FX-TBL-BASIC.docx", "resize-table-save");
    r.step(&json!({ "drag": { "from": { "table": 1, "row": 0, "border": 1 }, "to": { "table": 1, "row": 0, "border": 1, "dx": 30 } } }));
    r.step(
        &json!({ "drag": { "from": { "table": 1, "row": 1, "border": "bottom" }, "to": { "table": 1, "row": 1, "border": "bottom", "dy": 20 } } }),
    );
    let edited = tables(&r.e);
    assert_eq!(edited[1].0, vec![186.0, 126.0, 156.0]);
    assert!(edited[1].1[1].as_ref().is_some_and(|(_, rule)| rule == "atLeast"), "{:?}", edited[1].1);
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    // For opening the saved file in Word: RT_KEEP names a folder to write it to.
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-TBL-BASIC.resize.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    let back = tables(&again);
    assert_eq!(back[1].0, edited[1].0);
    let h = |v: &Option<(f64, String)>| v.as_ref().map(|(h, r)| ((h * 20.0).round(), r.clone()));
    assert_eq!(back[1].1.iter().map(h).collect::<Vec<_>>(), edited[1].1.iter().map(h).collect::<Vec<_>>());
    assert_eq!(back[1].2, edited[1].2);
    assert_eq!(texts(&again), texts(&r.e));
}

#[test]
fn a_reanchored_float_saves_in_its_new_paragraph() {
    // The drawing leaves its old paragraph and is written at the start of the new one, its offset patched.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-OBJ.docx", "reanchor-save");
    r.step(&json!({ "drag": { "from": { "float": 1 }, "to": { "float": 1, "dy": -150 } } }));
    let floats = |e: &mut super::super::Editor| crate::editing::inspect::inspect(e, "dump", true)["dump"]["floats"].clone();
    let edited = floats(&mut r.e);
    assert_eq!(edited[0]["anchor"], json!(5), "{edited}");
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-OBJ.reanchor.docx"), &bytes).expect("writes");
    }
    let mut again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    // Offsets read back within the file's rounding (EMU, 1/20 pt).
    let back = floats(&mut again);
    for (a, b) in back.as_array().unwrap().iter().zip(edited.as_array().unwrap()) {
        for k in ["anchor", "wrap", "w", "h"] {
            assert_eq!(a[k], b[k], "{back}");
        }
        for k in ["x", "y"] {
            assert!((a[k].as_f64().unwrap() - b[k].as_f64().unwrap()).abs() < 0.1, "{back}");
        }
    }
    assert_eq!(texts(&again), texts(&r.e));
}

#[test]
fn a_border_moved_in_selected_rows_saves() {
    // The grid gains a column; spans, widths and the grid are written and read back.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-TBL-BASIC.docx", "obs48-save");
    r.step(&json!({ "click": { "para": 13, "offset": 2 } }));
    r.step(&json!({"command": {"type": "select", "value": "row"}}));
    r.step(&json!({ "drag": { "from": { "table": 1, "row": 0, "border": 1 }, "to": { "table": 1, "row": 0, "border": 1, "dx": 30 } } }));
    let tables = |e: &mut super::super::Editor| crate::editing::inspect::inspect(e, "dump", true)["dump"]["tables"].clone();
    let edited = tables(&mut r.e);
    assert_eq!(edited[1]["grid"], json!([156.0, 30.0, 126.0, 156.0]));
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-TBL-BASIC.obs48.docx"), &bytes).expect("writes");
    }
    let mut again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(tables(&mut again), edited);
    assert_eq!(texts(&again), texts(&r.e));
}

#[test]
fn table_formats_save_and_reopen() {
    // Cell alignment, shading and header rows are written as direct properties and read back.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-TBL-BASIC.docx", "table-format-save");
    r.step(&json!({ "drag": { "from": { "para": 13, "offset": 2 }, "to": { "para": 17, "offset": 2 } } }));
    r.step(&json!({"command": {"type": "setCellAlignment", "value": "bottom"}}));
    r.step(&json!({"command": {"type": "setCellShading", "value": "#DDEBF7"}}));
    r.step(&json!({ "click": { "para": 13, "offset": 2 } }));
    r.step(&json!({"command": {"type": "setHeaderRow", "value": true}}));
    let tables = |e: &mut super::super::Editor| crate::editing::inspect::inspect(e, "dump", true)["dump"]["tables"].clone();
    let edited = tables(&mut r.e);
    assert_eq!(edited[1]["header_rows"], json!(1));
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-TBL-BASIC.formats.docx"), &bytes).expect("writes");
    }
    let mut again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(tables(&mut again), edited);
    assert_eq!(texts(&again), texts(&r.e));
}

#[test]
fn an_inserted_footnote_saves_and_deleting_its_reference_deletes_it() {
    // Insert Footnote puts the caret in the new note; the reference and the note are written; deleting the
    // reference deletes the note in the same step, and saving drops it.
    let note_texts = |e: &super::super::Editor| -> Vec<String> {
        let mut v: Vec<(i64, String)> = e
            .document()
            .footnotes
            .iter()
            .map(|(id, b)| {
                (
                    *id,
                    b.iter()
                        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(super::super::text::UnitText::of(p).text) } else { None })
                        .collect::<String>(),
                )
            })
            .collect();
        v.sort();
        v.into_iter().map(|(_, t)| t).collect()
    };
    let mut e = open("FX-STORIES.docx");
    let before = note_texts(&e);
    let para = super::super::tests::para_id(&e, 1);
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(4))));
    e.execute(Command::InsertFootnote, fonts());
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(StoryId::Footnote(_))), "{:?}", e.selection());
    type_text(&mut e, "New note.");
    let edited = note_texts(&e);
    assert_eq!(edited.len(), before.len() + 1);
    assert!(edited.last().unwrap().ends_with(" New note."), "{edited:?}");
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-STORIES.footnote.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(note_texts(&again), edited);
    assert_eq!(texts(&again), texts(&e));
    // Back in the body, Delete over the reference mark deletes the note with it; undo brings both back.
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(4))));
    e.execute(crate::editing::tests::del(super::super::commands::Dir::Forward, false), fonts());
    assert_eq!(note_texts(&e), before);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(note_texts(&again), before);
    e.execute(Command::Undo, fonts());
    assert_eq!(note_texts(&e), edited);
}

#[test]
fn notes_inserted_in_a_document_without_notes_save() {
    // The notes parts, their relationships and content types are created.
    let mut e = open("FX-TEXT.docx");
    assert!(e.document().footnotes.is_empty() && e.document().endnotes.is_empty());
    let para = super::super::tests::para_id(&e, 1);
    for kind in [crate::docx::NoteKind::Foot, crate::docx::NoteKind::End] {
        e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(3))));
        e.execute(if kind == crate::docx::NoteKind::Foot { Command::InsertFootnote } else { Command::InsertEndnote }, fonts());
        type_text(&mut e, "A note.");
    }
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-TEXT.notes.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    let text = |m: &std::collections::HashMap<i64, Vec<crate::docx::Block>>| -> Vec<String> {
        m.values()
            .flat_map(|b| b.iter())
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(super::super::text::UnitText::of(p).text) } else { None })
            .collect()
    };
    assert_eq!(text(&again.document().footnotes), vec!["\u{FFFC} A note.".to_string()]);
    assert_eq!(text(&again.document().endnotes), vec!["\u{FFFC} A note.".to_string()]);
    assert_eq!(texts(&again), texts(&e));
}

#[test]
fn deleted_and_split_cells_save_and_reopen() {
    // Delete Cells shifting left (the row ends early) and Split Cells into rows (vertical merges) are written.
    use serde_json::json;
    for cmd in [json!({ "type": "deleteCells", "value": "left" }), json!({ "type": "splitCells", "columns": 1, "rows": 2 })] {
        let mut r = super::scenarios::Run::open("FX-TBL-BASIC.docx", "cells-save");
        r.step(&json!({ "click": { "para": 14, "offset": 2 } }));
        r.step(&json!({ "command": cmd }));
        let tables = |e: &mut super::super::Editor| crate::editing::inspect::inspect(e, "dump", true)["dump"]["tables"].clone();
        let merges = |e: &super::super::Editor| -> Vec<Vec<crate::docx::VMerge>> {
            e.document().sections[0]
                .blocks
                .iter()
                .filter_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None })
                .nth(1)
                .unwrap()
                .rows
                .iter()
                .map(|r| r.cells.iter().map(|c| c.v_merge).collect())
                .collect()
        };
        let edited = tables(&mut r.e);
        let bytes = crate::docx::write::save(r.e.document()).expect("saves");
        if let Some(d) = std::env::var_os("RT_KEEP") {
            std::fs::write(std::path::Path::new(&d).join(format!("FX-TBL-BASIC.{cmd}.docx")), &bytes).expect("writes");
        }
        let mut again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
        assert_eq!(tables(&mut again), edited, "{cmd}");
        assert_eq!(merges(&again), merges(&r.e), "{cmd}");
        assert_eq!(texts(&again), texts(&r.e), "{cmd}");
    }
}

#[test]
fn editing_works_in_every_layout_status() {
    // A stabilized or inconsistent layout is reported (inspect `sync.status`), and typing, undo and saving work.
    use crate::layout::document::{FAIL_VERIFY, HF_ALTERNATE};
    // The layout gate's fixtures (`TWIP_REFERENCE_FIXTURES`); skipped where they aren't.
    let path = std::env::var_os("TWIP_REFERENCE_FIXTURES").map(std::path::PathBuf::from).unwrap_or_default().join("HF9").join("HF9_f_pagenum.docx");
    let Ok(bytes) = std::fs::read(path) else { return };
    for (fail, want) in [(false, "stabilized"), (true, "inconsistent")] {
        HF_ALTERNATE.set(12.0);
        FAIL_VERIFY.set(fail);
        let mut e = super::super::Editor::open(&bytes, fonts()).expect("opens");
        e.complete(fonts());
        let status = |e: &mut super::super::Editor| crate::editing::inspect::inspect(e, "sync", true)["sync"]["status"].clone();
        assert_eq!(status(&mut e), serde_json::json!(want));
        let before = texts(&e);
        let para = super::super::tests::para_id(&e, 0);
        e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(0))));
        type_text(&mut e, "Still editable. ");
        e.complete(fonts());
        assert!(texts(&e)[0].starts_with("Still editable. "));
        assert!(crate::docx::write::save(e.document()).is_ok());
        e.execute(Command::Undo, fonts());
        assert_eq!(texts(&e), before);
        HF_ALTERNATE.set(0.0);
        FAIL_VERIFY.set(false);
    }
}

/// big.docx (FX-LARGE): opened without waiting for background layout.
fn open_large() -> super::super::Editor {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/big.docx")).expect("big.docx");
    super::super::Editor::open(&bytes, fonts()).expect("opens")
}

#[test]
fn down_right_after_enter_lands_where_the_finished_layout_puts_it() {
    // Enter near the start, then Down at once (no background layout between): the caret is where it is when
    // layout has finished first.
    let run = |settle: bool| {
        let mut e = open_large();
        if settle {
            e.complete(fonts());
        }
        let para = super::super::tests::para_id(&e, 2);
        e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(10))));
        e.execute(Command::InsertParagraph, fonts());
        if settle {
            e.complete(fonts());
        }
        e.execute(crate::editing::tests::mv(super::super::commands::MoveBy::Line, super::super::commands::Dir::Forward, false), fonts());
        // As a paragraph number and offset: ids differ between opens.
        let (p, u) = e.selection().focus().and_then(|f| f.para_offset()).expect("a caret");
        ((0..texts(&e).len()).position(|k| super::super::tests::para_id(&e, k) == p), u)
    };
    assert_eq!(run(false), run(true));
}

#[test]
fn a_late_selection_keeps_its_characters_through_an_early_reflow() {
    // A selection far down; an edit at the start reflows every page; the selection covers the same text.
    let mut e = open_large();
    e.complete(fonts());
    let n = texts(&e).len();
    let k = (n * 9 / 10..n).find(|&k| texts(&e)[k].chars().count() > 20).expect("a long late paragraph");
    let para = super::super::tests::para_id(&e, k);
    let at = |u| super::super::position::Position::text(StoryId::Body, para, Unit(u));
    e.set_caret_for_benchmark(Selection::range(at(3), at(15), crate::editing::selection::Granularity::Char));
    let before = e.selected_text();
    let first = super::super::tests::para_id(&e, 0);
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, first, Unit(0))));
    for _ in 0..5 {
        e.execute(Command::InsertParagraph, fonts());
    }
    let caret = e.selection();
    e.set_caret_for_benchmark(Selection::range(at(3), at(15), crate::editing::selection::Granularity::Char));
    assert_eq!(e.selected_text(), before, "the late paragraph's text moved ({caret:?})");
    e.complete(fonts());
    assert_eq!(e.selected_text(), before);
}

#[test]
fn a_click_on_a_provisional_page_resolves_against_the_finished_layout() {
    // After an edit near the start, the later pages are provisional; a click on one lays out through it first
    // and resolves where it does once layout has finished.
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    let edited = |settle: bool| {
        let mut e = open_large();
        e.complete(fonts());
        let para = super::super::tests::para_id(&e, 1);
        e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(0))));
        for _ in 0..3 {
            e.execute(Command::InsertParagraph, fonts());
        }
        if settle {
            e.complete(fonts());
        }
        e
    };
    let mut done = edited(true);
    let stack = done.stack();
    let (page, g) = done.page_geometries().into_iter().find(|(p, _)| *p >= 3).expect("a fourth page");
    let l = g.lines.iter().find(|l| l.stops.len() > 3 && g.stories[l.story].story == StoryId::Body).expect("a body line");
    let (x, y) = stack.to_doc(page, (l.stops[1].x + l.stops[2].x) / 2.0, (l.top + l.bottom) / 2.0).expect("on the stack");
    let click = |e: &mut super::super::Editor| {
        for kind in [PointerKind::Down, PointerKind::Up] {
            e.pointer_input(kind, x, y, 1, Mods::default(), fonts());
        }
        let (p, u) = e.selection().focus().and_then(|f| f.para_offset()).expect("a caret");
        ((0..texts(e).len()).position(|k| super::super::tests::para_id(e, k) == p), u)
    };
    let want = click(&mut done);
    let mut fresh = edited(false);
    assert!(fresh.snapshot().info().get(page).is_none_or(|i| i.provisional), "page {page} is provisional after the edit");
    assert_eq!(click(&mut fresh), want);
}

#[test]
fn a_row_inserted_in_a_repeating_section_saves_as_its_own_item() {
    // The section's wrappers are written around its rows again, the new row its own item.
    use crate::docx::ControlKind;
    let shape = |e: &super::super::Editor| -> Vec<Vec<String>> {
        let d = e.document();
        let t = d.sections[0].blocks.iter().find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).unwrap();
        t.rows.iter().map(|r| r.controls.iter().map(|id| format!("{:?}", d.controls.get(id).map(|c| c.props.kind.clone()))).collect()).collect()
    };
    let distinct_items = |e: &super::super::Editor| -> usize {
        let d = e.document();
        let t = d.sections[0].blocks.iter().find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).unwrap();
        let mut ids: Vec<crate::docx::ControlId> = t
            .rows
            .iter()
            .flat_map(|r| r.controls.iter().copied())
            .filter(|id| d.controls.get(id).is_some_and(|c| c.props.kind == ControlKind::RepeatingItem))
            .collect();
        ids.sort();
        ids.dedup();
        ids.len()
    };
    let mut e = open("FX-REPEAT-REGION.docx");
    assert_eq!(distinct_items(&e), 3);
    let para = super::super::tests::para_id(&e, 8);
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(2))));
    e.execute(
        Command::InsertRows(crate::api::command::InsertRows {
            side: if true { crate::api::command::VerticalSide::Below } else { crate::api::command::VerticalSide::Above },
            count: Some(1),
        }),
        fonts(),
    );
    type_text(&mut e, "Item 2b");
    let edited = shape(&e);
    assert_eq!(edited.len(), 6);
    assert_eq!(edited[3], vec!["Some(RepeatingSection)".to_string(), "Some(RepeatingItem)".to_string()]);
    assert_eq!(distinct_items(&e), 4);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-REPEAT-REGION.row.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(shape(&again), edited);
    assert_eq!(distinct_items(&again), 4);
    assert_eq!(texts(&again), texts(&e));
}

#[test]
fn a_toggled_checkbox_saves_its_state() {
    // `w14:checked` and the face are written; the control is kept.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-REPEAT-REGION.docx", "checkbox-save");
    r.step(&json!({ "click": { "para": 7, "offset": 0 } }));
    let faces = |e: &super::super::Editor| texts(e).into_iter().filter(|t| t.ends_with(" done")).collect::<Vec<_>>();
    let edited = faces(&r.e);
    assert_eq!(edited, vec!["\u{2612} done", "\u{2612} done", "\u{2610} done"]);
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-REPEAT-REGION.checkbox.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(faces(&again), edited);
    let boxes = again.document().controls.values().filter(|c| matches!(c.props.kind, crate::docx::ControlKind::Checkbox { .. })).count();
    assert_eq!(boxes, 3);
}

/// The body's floats in document order (they're the only kind FX-OBJ's body has besides inline pictures).
fn body_floats(e: &super::super::Editor) -> Vec<crate::docx::Float> {
    e.document()
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let crate::docx::Inline::Float(f, _) = i { Some((**f).clone()) } else { None })
        .collect()
}

fn body_pictures(e: &super::super::Editor) -> Vec<(f64, f64)> {
    e.document()
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let crate::docx::Inline::Picture(p, _) = i { Some((p.w, p.h)) } else { None })
        .collect()
}

#[test]
fn wrapping_and_the_anchor_lock_save() {
    // Wrap Text and Lock Anchor patch the drawing (its wrap element, `behindDoc`, `locked`) in place.
    use crate::editing::selection::ObjectRef;
    let mut e = open("FX-OBJ.docx");
    let ids: Vec<crate::layout::FloatId> = body_floats(&e).iter().map(|f| crate::layout::FloatId(f.id)).collect();
    let was = body_floats(&e);
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(ids[0])));
    assert!(matches!(e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::Tight), fonts()), super::super::Outcome::Changed(_)));
    e.execute(Command::SetAnchorLocked(true), fonts());
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(ids[1])));
    e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::BehindText), fonts());
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(ids[2])));
    e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::Square), fonts());
    let now = body_floats(&e);
    assert_eq!((now[0].wrap.as_str(), now[0].locked), ("tight", !was[0].locked));
    assert_eq!((now[1].wrap.as_str(), now[1].behind), ("none", true));
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-OBJ.wrap.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    let back = body_floats(&again);
    for (a, b) in back.iter().zip(&now) {
        assert_eq!((&a.wrap, &a.wrap_text, a.behind, a.locked), (&b.wrap, &b.wrap_text, b.behind, b.locked));
    }
    assert_eq!(texts(&again), texts(&e));
    // One step each: undoing all four gives the document back.
    for _ in 0..4 {
        e.execute(Command::Undo, fonts());
    }
    let undone = body_floats(&e);
    assert!(undone.iter().zip(&was).all(|(a, b)| a.wrap == b.wrap && a.behind == b.behind && a.locked == b.locked));
}

#[test]
fn an_inline_picture_resizes_by_its_handle_and_saves() {
    // A corner handle drag keeps a locked picture's ratio; saving patches its extents in place.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-OBJ-LOCK.docx", "inline-resize");
    let was = body_pictures(&r.e);
    r.step(&json!({ "click": { "picture": 0 } }));
    assert!(matches!(r.e.selection(), Selection::Object(crate::editing::selection::ObjectRef::Inline { .. })), "{:?}", r.e.selection());
    r.step(&json!({ "drag": { "from": { "picture": 0, "at": "bottom-right" }, "to": { "picture": 0, "at": "bottom-right", "dx": 36.0 } } }));
    let now = body_pictures(&r.e);
    assert!(now[0].0 > was[0].0 + 30.0, "{was:?} -> {now:?}");
    assert!((now[0].0 / now[0].1 - was[0].0 / was[0].1).abs() < 0.02, "ratio kept: {was:?} -> {now:?}");
    assert_eq!(&now[1..], &was[1..]);
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-OBJ.inline-resize.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    let back = body_pictures(&again);
    assert!(back.iter().zip(&now).all(|(a, b)| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01), "{back:?} vs {now:?}");
    assert_eq!(texts(&again), texts(&r.e));
}

#[test]
fn reset_size_gives_a_picture_its_own_size() {
    // Reset Picture size: the image's pixels at its resolution (96 dpi without one).
    use crate::editing::selection::ObjectRef;
    let mut e = open("FX-OBJ.docx");
    let (para, offset) = e
        .index()
        .para_ids()
        .find_map(|id| {
            let p = e.index().para(e.document(), id)?;
            let mut at = 0u32;
            p.inlines.iter().find_map(|i| {
                let here = at;
                at += i.units() as u32;
                matches!(i, crate::docx::Inline::Picture(..)).then_some((id, Unit(here)))
            })
        })
        .expect("an inline picture");
    let pic = |e: &super::super::Editor| match e.index().para(e.document(), para).and_then(|p| super::super::stories::inline_at(p, offset)) {
        Some(crate::docx::Inline::Picture(p, _)) => (p.w, p.h, p.image),
        _ => panic!("no picture"),
    };
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Inline { para, offset }));
    e.execute(
        Command::SetObjectSize(crate::api::command::ObjectSize {
            width: Some(crate::api::value::Points(20.0)),
            height: Some(crate::api::value::Points(10.0)),
            lock_aspect: None,
        }),
        fonts(),
    );
    assert_eq!((pic(&e).0, pic(&e).1), (20.0, 10.0));
    e.execute(Command::ResetObjectSize, fonts());
    let image = pic(&e).2;
    let (w, h, dx, dy) = crate::docx::image::image_info(&e.document().images[image.expect("an image")]).expect("decodes");
    let want = ((w * 72.0 / dx * 20.0).round() / 20.0, (h * 72.0 / dy * 20.0).round() / 20.0);
    assert_eq!((pic(&e).0, pic(&e).1), want);
    // Wrapping an inline picture isn't done: it says so and changes nothing.
    assert!(matches!(e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::Square), fonts()), super::super::Outcome::Refused(_)));
}

#[test]
fn an_inserted_picture_saves_with_its_image() {
    // Insert Picture puts the image at the caret, selected; saving writes the drawing, the media part, its
    // relationship and content type, in the body and in a header (each part its own relationships).
    use crate::editing::selection::ObjectRef;
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut e = open("FX-STORIES.docx");
    let out =
        e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data.clone()), alt_text: None }), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert!(matches!(e.selection(), Selection::Object(ObjectRef::Inline { .. })), "{:?}", e.selection());
    // And one in the first header: its first paragraph's start.
    let header = e.document().stories.keys().filter(|n| n.contains("header")).min().cloned().expect("a header");
    let story = e.index().header_footer(&header).expect("its story");
    let para =
        e.index().para_ids().find(|id| e.index().para_slot(*id).is_some_and(|s| e.index().story(s.container) == story)).expect("a header paragraph");
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(story, para, Unit(0))));
    e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data.clone()), alt_text: None }), fonts());
    // Not an image: nothing happens.
    assert!(matches!(
        e.execute(
            Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(b"not an image".to_vec()), alt_text: None }),
            fonts()
        ),
        super::super::Outcome::Unchanged(_)
    ));
    let pictures = |e: &super::super::Editor| -> Vec<(f64, f64, Vec<u8>)> {
        let d = e.document();
        let mut out = Vec::new();
        let paras = d.sections.iter().flat_map(|s| s.blocks.iter()).chain(d.stories.values().flatten());
        for b in paras {
            if let crate::docx::Block::Para(p) = b {
                for i in &p.inlines {
                    if let crate::docx::Inline::Picture(pic, _) = i {
                        out.push((pic.w, pic.h, pic.image.map(|k| d.images[k].clone()).unwrap_or_default()));
                    }
                }
            }
        }
        out
    };
    let now = pictures(&e);
    assert_eq!(now.len(), 2, "{:?}", now.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>());
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-STORIES.picture.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    let back = pictures(&again);
    assert_eq!(back.len(), 2);
    for (a, b) in back.iter().zip(&now) {
        assert!((a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01 && a.2 == b.2, "{:?} vs {:?}", (a.0, a.1), (b.0, b.1));
    }
    assert_eq!(texts(&again), texts(&e));
    // Undo takes both out.
    e.execute(Command::Undo, fonts());
    e.execute(Command::Undo, fonts());
    assert!(pictures(&e).is_empty());
}

#[test]
fn an_inserted_text_box_takes_typing_and_saves() {
    // Insert Text Box anchors a new box at the caret's paragraph with the caret in it; typing goes in;
    // saving writes the anchor and its content from the model, and reading it back gives the box and its text.
    let mut e = open("FX-TEXT.docx");
    let before = body_floats(&e).len();
    assert!(matches!(e.execute(Command::InsertTextBox, fonts()), super::super::Outcome::Changed(_)));
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(StoryId::TextBox(_))), "{:?}", e.selection());
    e.execute(Command::InsertText("Boxed text".into()), fonts());
    let box_text = |e: &super::super::Editor| -> Vec<String> {
        body_floats(e)
            .iter()
            .filter_map(|f| f.text_box.as_ref())
            .map(|tb| {
                tb.blocks
                    .iter()
                    .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
                    .flat_map(|p| p.inlines.iter())
                    .filter_map(|i| if let crate::docx::Inline::Text(t, _) = i { Some(t.to_string()) } else { None })
                    .collect()
            })
            .collect()
    };
    assert_eq!(body_floats(&e).len(), before + 1);
    assert_eq!(box_text(&e), vec!["Boxed text".to_string()]);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-TEXT.textbox.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(box_text(&again), box_text(&e));
    let (a, b) = (&body_floats(&again)[before], &body_floats(&e)[before]);
    assert!((a.w - b.w).abs() < 0.01 && (a.h - b.h).abs() < 0.01 && a.wrap == b.wrap && a.h_relative == b.h_relative, "{a:?}");
    assert_eq!(texts(&again), texts(&e));
    // Undo: the typing, then the box.
    e.execute(Command::Undo, fonts());
    e.execute(Command::Undo, fonts());
    assert_eq!(body_floats(&e).len(), before);
}

fn same_lines(a: &[String], b: &[String]) {
    if let Some(k) = (0..a.len().max(b.len())).find(|&k| a.get(k) != b.get(k)) {
        panic!(
            "line {k} differs:
  got  {:?}
  want {:?}",
            a.get(k),
            b.get(k)
        );
    }
}

/// Every body table's own and its rows' and cells' formatting, comparable across a save.
fn table_formatting(e: &super::super::Editor) -> Vec<String> {
    fn walk(blocks: &[crate::docx::Block], out: &mut Vec<String>) {
        for b in blocks {
            if let crate::docx::Block::Table(t) = b {
                out.push(format!("table {:?} {:?} {:?} {:?} {:?} grid {:?}", t.style_id, t.look, t.width, t.jc, (t.ind * 20.0).round(), t.grid));
                for r in &t.rows {
                    out.push(format!("  row {:?} cant_split {}", r.height, r.cant_split));
                    for c in &r.cells {
                        out.push(format!(
                            "    cell {:?} {:?} {:?} {:?} {:?} {:?} {:?} {}",
                            c.width, c.top, c.left, c.bottom, c.right, c.shading, c.margins, c.v_align
                        ));
                        walk(&c.blocks, out);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for s in &e.document().sections {
        walk(&s.blocks, &mut out);
    }
    out
}

#[test]
fn table_properties_save_and_reopen() {
    // Borders (shared edges kept consistent), margins, row height, can't split, column width, and the
    // table's width, alignment and indent, each one step, written in place and read back.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-TBL-BASIC.docx", "table-properties-save");
    let before = table_formatting(&r.e);
    r.step(&json!({ "drag": { "from": { "para": 13, "offset": 2 }, "to": { "para": 17, "offset": 2 } } }));
    let steps = [
        json!({ "type": "setCellBorders", "edges": "outside", "border": { "style": "double", "width": 1.5, "color": "#C00000" } }),
        json!({ "type": "setCellBorders", "edges": "insideHorizontal", "border": { "style": "dashed", "width": 0.75, "color": "#0070C0" } }),
        json!({ "type": "setCellMargins", "top": 2, "left": 9, "bottom": 2, "right": 9 }),
        json!({ "type": "setRowHeight", "value": { "height": 30, "rule": "exact" } }),
        json!({ "type": "setRowCanBreak", "value": false }),
        json!({ "type": "setColumnWidth", "value": 100 }),
        json!({ "type": "setTableAlignment", "value": "center" }),
        json!({ "type": "setTableIndent", "value": 18 }),
    ];
    for op in steps.clone() {
        let undo = r.e.history().undo_depth();
        r.step(&json!({ "command": op }));
        assert_eq!(r.e.history().undo_depth(), undo + 1, "{op}: one step");
    }
    let edited = table_formatting(&r.e);
    assert_ne!(edited, before);
    // A block side on the outside also sets the facing side of the cell beyond it (the row above's bottom).
    let t =
        r.e.document().sections[0].blocks.iter().filter_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).nth(1).unwrap();
    let top = t.rows.iter().position(|row| row.height.as_ref().is_some_and(|h| h.1 == "exact")).expect("a row made exact");
    if top > 0 {
        let above = &t.rows[top - 1].cells[1];
        assert_eq!(above.bottom.as_ref().map(|b| b.style.as_str()), Some("double"), "{above:?}");
    }
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-TBL-BASIC.properties.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    same_lines(&table_formatting(&again), &edited);
    assert_eq!(texts(&again), texts(&r.e));
    // Undoing every step gives the tables back.
    for _ in 0..steps.len() {
        r.e.execute(Command::Undo, fonts());
    }
    same_lines(&table_formatting(&r.e), &before);
}

#[test]
fn a_table_style_and_its_options_restyle_the_cells() {
    // Table Style and Table Style Options: every cell (and its paragraphs) re-resolved from the style's
    // regions; saved as tblStyle and tblLook, read back the same.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("../tables.docx", "table-style-save");
    let first_table = |e: &super::super::Editor| -> crate::docx::Table {
        e.document()
            .sections
            .iter()
            .flat_map(|s| s.blocks.iter())
            .find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t.clone()) } else { None })
            .unwrap()
    };
    let t0 = first_table(&r.e);
    assert_eq!(t0.style_id.as_deref(), Some("TableGrid"));
    // A click in the first table's first cell: its paragraph's number is the count of body paragraphs before it.
    let para = r.e.document().sections[0].blocks.iter().take_while(|b| matches!(b, crate::docx::Block::Para(_))).count();
    r.step(&json!({ "click": { "para": para, "offset": 0 } }));
    r.step(&json!({"command": {"type": "setTableStyle", "value": "GridTable4-Accent1"}}));
    let styled = first_table(&r.e);
    assert_eq!(styled.style_id.as_deref(), Some("GridTable4-Accent1"));
    let fill = |t: &crate::docx::Table, row: usize| t.rows[row].cells[0].shading;
    assert_ne!(fill(&styled, 0), fill(&t0, 0), "the header row takes the style's fill");
    r.step(&json!({"command": {"type": "setTableStyleOptions", "headerRow": false, "totalRow": false, "firstColumn": false, "lastColumn": false, "bandedRows": true, "bandedColumns": false}}));
    let plain = first_table(&r.e);
    assert_ne!(fill(&plain, 0), fill(&styled, 0), "no header row: the first row is a band");
    let edited = table_formatting(&r.e);
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("tables.style.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    same_lines(&table_formatting(&again), &edited);
    assert_eq!(texts(&again), texts(&r.e));
}

/// Each header and footer part's text, by name.
fn story_texts(e: &super::super::Editor) -> Vec<(String, String)> {
    e.document()
        .stories
        .iter()
        .map(|(n, b)| {
            (
                n.clone(),
                b.iter()
                    .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(super::super::text::UnitText::of(p).text) } else { None })
                    .collect::<Vec<_>>()
                    .join("|"),
            )
        })
        .collect()
}

#[test]
fn a_new_header_and_footer_save() {
    // Edit Header and Edit Footer on a document without them make each (one step), with the caret
    // in it; saving writes the parts, their relationships, content types and the section's references.
    let mut e = open("FX-TEXT.docx");
    assert!(e.document().stories.is_empty());
    assert!(matches!(e.execute(Command::EditHeader, fonts()), super::super::Outcome::Changed(_)));
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(StoryId::HeaderFooter(_))), "{:?}", e.selection());
    type_text(&mut e, "New header");
    e.execute(Command::CloseHeaderFooter, fonts());
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(StoryId::Body)));
    e.execute(Command::EditFooter, fonts());
    type_text(&mut e, "New footer");
    // Edit Header again goes into the header made, not a new one.
    e.execute(Command::EditHeader, fonts());
    let edited = story_texts(&e);
    assert_eq!(edited.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), vec!["New footer", "New header"], "{edited:?}");
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-TEXT.header.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(story_texts(&again), edited);
    assert!(again.document().sections[0].headers[0].is_some() && again.document().sections[0].footers[0].is_some());
    assert_eq!(texts(&again), texts(&e));
    // Undo takes the typing and the parts out again.
    for _ in 0..4 {
        e.execute(Command::Undo, fonts());
    }
    assert!(e.document().stories.is_empty(), "{:?}", story_texts(&e));
}

#[test]
fn a_double_click_in_an_empty_header_area_makes_the_header() {
    // A double-click in the top margin of a page without a header enters a new one; Esc goes back.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-PAGES.docx", "new-header-click");
    r.step(&json!({ "click": { "para": 0, "offset": 0, "dy": -40 }, "clicks": 2 }));
    assert!(matches!(r.e.selection().focus().map(|p| p.story), Some(StoryId::HeaderFooter(_))), "{:?}", r.e.selection());
    assert_eq!(r.e.document().stories.len(), 1);
    r.step(&json!({ "key": "Escape" }));
    assert!(matches!(r.e.selection().focus().map(|p| p.story), Some(StoryId::Body)));
}

#[test]
fn new_lists_save_with_their_definitions() {
    // Lists made by Numbering and Bullets are written with their definitions: into the numbering part
    // when there is one, into a new part (with its relationship and content type) when there isn't.
    let labels = |e: &super::super::Editor| -> Vec<Option<String>> {
        e.document()
            .sections
            .iter()
            .flat_map(|s| s.blocks.iter())
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.list.as_ref().map(|l| l.text.clone())) } else { None })
            .collect()
    };
    for (fixture, name) in [("FX-TEXT.docx", "FX-TEXT.lists.docx"), ("FX-LISTS.docx", "FX-LISTS.lists.docx")] {
        let mut e = open(fixture);
        let first = super::super::tests::para_id(&e, 1);
        e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, first, Unit(0))));
        assert!(
            matches!(e.execute(Command::SetList(Some(crate::api::command::ListKind::Numbered)), fonts()), super::super::Outcome::Changed(_)),
            "{fixture}"
        );
        let last = super::super::tests::para_id(&e, e.document().sections[0].blocks.len() - 1);
        e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, last, Unit(0))));
        e.execute(Command::SetList(Some(crate::api::command::ListKind::Bullet)), fonts());
        let edited = labels(&e);
        assert_eq!(edited[1].as_deref(), Some("1."), "{fixture}: {edited:?}");
        let bytes = crate::docx::write::save(e.document()).expect("saves");
        if let Some(d) = std::env::var_os("RT_KEEP") {
            std::fs::write(std::path::Path::new(&d).join(name), &bytes).expect("writes");
        }
        let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
        assert_eq!(labels(&again), edited, "{fixture}");
        assert_eq!(texts(&again), texts(&e));
    }
}

#[test]
fn a_placeholder_activates_and_fills() {
    // A click on an unfilled placeholder selects it and reports it; filling it fits the image into
    // its box (one step); saving keeps the control's tag and alias, replaces the picture and clears showingPlcHdr.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-PLACEHOLDER.docx", "placeholder");
    // The placeholder's picture among those drawn.
    let st = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
    let pics = st["geometry"]["pictures"].as_array().cloned().unwrap_or_default();
    let k = (0..pics.len())
        .find(|&k| {
            r.step(&json!({ "click": { "picture": k } }));
            r.e.selected_placeholder().is_some()
        })
        .expect("a drawn placeholder");
    let info = r.e.selected_placeholder().unwrap();
    assert_eq!((info.key.as_str(), info.label.as_str()), ("Signature.Recipient Sig", "Recipient Sig"));
    // The click reported it.
    let pt = pics[k]["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>();
    let (x, y) = ((pt[0] + pt[2]) / 2.0, (pt[1] + pt[3]) / 2.0);
    let out = r.e.apply_json(&json!({ "type": "pointer", "kind": "down", "x": x, "y": y, "clicks": 1, "seq": 1000 }).to_string(), fonts());
    assert!(out.contains("\"placeholder\"") && out.contains("Signature.Recipient Sig"), "{out}");
    r.e.apply_json(&json!({ "type": "pointer", "kind": "up", "x": x, "y": y, "seq": 1001 }).to_string(), fonts());
    // Fill every placeholder with that key: a 108 x 72 pt picture fits the 214.15 x 39.65 box by its height.
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let out = r.e.execute(
        Command::FillPlaceholder(crate::api::command::FillPlaceholder { key: Some(info.key.clone()), image: crate::api::value::Bytes(data.clone()) }),
        fonts(),
    );
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert!(r.e.selected_placeholder().is_none(), "filled: no longer a placeholder");
    let filled: Vec<(f64, f64)> =
        r.e.document()
            .sections
            .iter()
            .flat_map(|s| s.blocks.iter())
            .flat_map(|b| match b {
                crate::docx::Block::Para(p) => vec![p.clone()],
                crate::docx::Block::Table(t) => t
                    .rows
                    .iter()
                    .flat_map(|r| r.cells.iter())
                    .flat_map(|c| c.blocks.iter())
                    .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.clone()) } else { None })
                    .collect(),
            })
            .flat_map(|p| p.inlines.into_iter())
            .filter_map(|i| {
                if let crate::docx::Inline::Picture(p, _) = i {
                    (p.image.is_some_and(|k| r.e.document().filled.contains(&k))).then_some((p.w, p.h))
                } else {
                    None
                }
            })
            .collect();
    assert!(!filled.is_empty());
    let (w, h) = filled[0];
    assert!((h - 39.65).abs() < 0.1 && w < 214.15, "fitted by height: {w} x {h}");
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-PLACEHOLDER.filled.docx"), &bytes).expect("writes");
    }
    let xml = {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).unwrap();
        let mut s = String::new();
        z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
        s
    };
    assert!(xml.contains("Placeholder:Signature.Recipient Sig") && xml.contains("Recipient Sig"), "tag and alias kept");
    assert!(!xml.contains("showingPlcHdr"), "no longer showing its placeholder");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert!(again.document().images.contains(&data), "the filled image is saved");
    assert!(again.document().placeholders.is_empty(), "read back as filled");
    assert_eq!(texts(&again), texts(&r.e));
    // Undo gives the placeholder back.
    r.e.execute(Command::Undo, fonts());
    assert!(r.e.document().filled.is_empty());
}

#[test]
fn the_host_sets_how_placeholders_look() {
    // A look for a key (or the default) puts the placeholder in the view with that colour, label and icon;
    // without one it isn't overlaid; a filled placeholder never is.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-PLACEHOLDER.docx", "placeholder-look");
    let views = |r: &mut super::scenarios::Run| {
        let n = r.e.stack().pages.len();
        r.e.view(0..n).placeholders
    };
    assert!(views(&mut r).is_empty(), "no look set: drawn as stored");
    let icon = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75z"/><circle cx="20" cy="4" r="2" fill="#333333"/><rect x="0" y="0" width="24" height="24" fill="none"/></svg>"##;
    let look = json!({ "fill": "#C00000", "label": "Sign here", "iconSvg": icon });
    r.step(&json!({ "command": { "type": "setPlaceholderLooks", "looks": [{ "match": { "tag": "Signature.Recipient Sig" }, "look": look }] } }));
    let v = views(&mut r);
    assert_eq!(v.len(), 1, "{v:?}");
    assert_eq!((v[0].label.as_str(), v[0].color), ("Sign here", 0x40_C0_00_00));
    // The icon's shapes: the path and the circle (its own fill); the unfilled rect is left out.
    let shapes = v[0].icon.as_ref().expect("an icon");
    assert_eq!((shapes.view_box, shapes.paths.len(), shapes.paths[1].1), ([0.0, 0.0, 24.0, 24.0], 2, Some(0xFF33_3333)));
    assert!(v[0].rect[2] - v[0].rect[0] > 200.0, "the placeholder's box: {:?}", v[0].rect);
    // The default look, with the alias as its label, for every other placeholder.
    r.step(&json!({ "command": { "type": "setPlaceholderLooks", "looks": [{ "match": "default", "look": { "fill": "#00AA00" } }] } }));
    let v = views(&mut r);
    assert_eq!((v.len(), v[0].label.as_str(), v[0].color), (1, "Recipient Sig", 0x40_00_AA_00));
    // Filled: not a placeholder any more.
    let data = open("FX-OBJ.docx").document().images.last().cloned().unwrap();
    r.e.execute(
        Command::FillPlaceholder(crate::api::command::FillPlaceholder {
            key: Some("Signature.Recipient Sig".into()),
            image: crate::api::value::Bytes(data),
        }),
        fonts(),
    );
    assert!(views(&mut r).is_empty());
}

/// The text inside each content control of a saved document, by tag (in document order).
fn controls_by_tag(bytes: &[u8]) -> Vec<(String, String)> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    z.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    let dom = roxmltree::Document::parse(&xml).unwrap();
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    dom.descendants()
        .filter(|n| n.has_tag_name((w, "sdt")))
        .map(|n| {
            let tag = n.descendants().find(|t| t.has_tag_name((w, "tag"))).and_then(|t| t.attribute((w, "val"))).unwrap_or("").to_string();
            let content = n.children().find(|c| c.has_tag_name((w, "sdtContent")));
            let mut text = String::new();
            for d in content.into_iter().flat_map(|c| c.descendants()) {
                if d.has_tag_name((w, "t")) {
                    text.push_str(d.text().unwrap_or(""));
                } else if d.has_tag_name((w, "p")) && !text.is_empty() {
                    text.push('|');
                }
            }
            (tag, text)
        })
        .collect()
}

#[test]
fn content_controls_survive_edits() {
    // Block, run and cell-level content controls keep their wrappers and hold what was
    // edited inside them.
    use serde_json::json;
    let run = |steps: &[serde_json::Value]| -> Vec<(String, String)> {
        let mut r = super::scenarios::Run::open("FX-CONTROLS.docx", "controls");
        for s in steps {
            r.step(s);
        }
        let bytes = crate::docx::write::save(r.e.document()).expect("saves");
        let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
        assert_eq!(texts(&again), texts(&r.e), "{steps:?}");
        controls_by_tag(&bytes)
    };
    let at = |p: usize, o: serde_json::Value| json!({ "click": { "para": p, "offset": o } });
    let cases: Vec<(&str, Vec<serde_json::Value>)> = vec![
        ("nothing", vec![]),
        ("type in the block control", vec![at(2, json!(3)), json!({ "type": "X" })]),
        ("Enter at the block control's end", vec![at(3, json!("end")), json!({ "key": "Enter" }), json!({ "type": "New" })]),
        (
            "delete a paragraph of the block control",
            vec![at(2, json!(0)), json!({ "key": "ArrowDown", "mods": { "shift": true } }), json!({ "key": "Delete" })],
        ),
        ("type in the text control", vec![at(4, json!(8)), json!({ "type": "X" })]),
        ("Enter after the text control", vec![at(4, json!("end")), json!({ "key": "Enter" }), json!({ "type": "Next" })]),
        ("type in the cell control", vec![at(5, json!("end")), json!({ "key": "Tab" }), json!({ "key": "End" }), json!({ "type": "X" })]),
        ("insert a row below the cell control's row", vec![at(5, json!(0)), json!({"command": {"type": "insertRows", "where": "below"}})]),
        ("Enter inside the block control", vec![at(2, json!(3)), json!({ "key": "Enter" }), json!({ "type": "Mid" })]),
        (
            "delete across the block control's start",
            vec![at(1, json!(3)), json!({ "key": "ArrowDown", "mods": { "shift": true } }), json!({ "key": "Delete" })],
        ),
        ("Backspace after the block control", vec![at(4, json!(0)), json!({ "key": "Backspace" })]),
        ("Backspace at the block control's start", vec![at(2, json!(0)), json!({ "key": "Backspace" })]),
        (
            "merge the cell control's row",
            vec![
                json!({ "drag": { "from": { "para": 5, "offset": 0 }, "to": { "para": 6, "offset": "end" } } }),
                json!({"command": {"type": "mergeCells"}}),
            ],
        ),
        ("insert a column right of the cell control", vec![at(6, json!(0)), json!({"command": {"type": "insertColumns", "where": "right"}})]),
        ("insert a column left of the cell control", vec![at(6, json!(0)), json!({"command": {"type": "insertColumns", "where": "left"}})]),
        ("insert a row above the cell control's row", vec![at(6, json!(0)), json!({"command": {"type": "insertRows", "where": "above"}})]),
        ("split the cell control", vec![at(6, json!(0)), json!({"command": {"type": "splitCells", "columns": 2}})]),
        ("split the cell control into rows", vec![at(6, json!(0)), json!({"command": {"type": "splitCells", "columns": 1, "rows": 2}})]),
        (
            "copy the name line to the end",
            vec![
                at(4, json!(0)),
                json!({ "key": "End", "mods": { "shift": true } }),
                json!({ "clipboard": "copy" }),
                at(9, json!("end")),
                json!({ "key": "Enter" }),
                json!({ "clipboard": "paste" }),
            ],
        ),
        (
            "cut and undo the block control",
            vec![
                at(2, json!(0)),
                json!({ "key": "ArrowDown", "mods": { "shift": true } }),
                json!({ "key": "End", "mods": { "shift": true } }),
                json!({ "clipboard": "cut" }),
                json!({"command": {"type": "undo"}}),
            ],
        ),
    ];
    let mut report = Vec::new();
    for (name, steps) in cases {
        report.push(format!("{name}: {:?}", run(&steps)));
    }
    let report = report.join("\n");
    eprintln!("{report}");
    for line in report.lines() {
        for tag in ["blk", "name", "cell"] {
            // A pasted copy of a whole control is a second control.
            let want = if tag == "name" && line.starts_with("copy the name line") { 2 } else { 1 };
            assert_eq!(line.matches(&format!("(\"{tag}\"")).count(), want, "control {tag} lost or doubled: {line}");
        }
    }
}

#[test]
fn placeholders_cant_be_resized_or_moved() {
    // A placeholder's picture (the control a click acts on) is fixed: selected, it shows no handles, a drag from its
    // corner doesn't resize it, and Size doesn't apply. Filled and read back, it stays fixed.
    use serde_json::json;
    let size = |e: &super::super::Editor| -> Vec<(f64, f64)> {
        let d = e.document();
        let mut v = Vec::new();
        for s in &d.sections {
            for b in &s.blocks {
                let paras: Vec<&crate::docx::Paragraph> = match b {
                    crate::docx::Block::Para(p) => vec![p],
                    crate::docx::Block::Table(t) => t
                        .rows
                        .iter()
                        .flat_map(|r| r.cells.iter())
                        .flat_map(|c| c.blocks.iter())
                        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
                        .collect(),
                };
                for p in paras {
                    for i in &p.inlines {
                        if let crate::docx::Inline::Picture(pic, _) = i
                            && pic.image.is_some_and(|k| d.fixed.contains(&k))
                        {
                            v.push((pic.w, pic.h));
                        }
                    }
                }
            }
        }
        v
    };
    let check = |mut r: super::scenarios::Run| {
        let st = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
        let pics = st["geometry"]["pictures"].as_array().cloned().unwrap_or_default();
        let k = (0..pics.len())
            .find(|&k| {
                r.step(&json!({ "click": { "picture": k } }));
                r.e.selected_placeholder().is_some() || matches!(r.e.view(0..1).selection, crate::editing::view::SelectionView::Object(_, false))
            })
            .expect("a fixed picture");
        let crate::editing::view::SelectionView::Object(_, handles) = r.e.view(0..1).selection else { panic!("an object selected") };
        assert!(!handles, "no handles");
        let before = size(&r.e);
        assert!(!before.is_empty());
        let corner = json!({ "picture": k, "at": "bottom-right" });
        let to = json!({ "picture": k, "at": "bottom-right", "dx": 40.0, "dy": 30.0 });
        r.step(&json!({ "drag": { "from": corner, "to": to } }));
        assert_eq!(size(&r.e), before, "a drag from the corner doesn't resize it");
        r.step(&json!({ "click": { "picture": k } }));
        let out = r.e.execute(
            Command::SetObjectSize(crate::api::command::ObjectSize {
                width: Some(crate::api::value::Points(20.0)),
                height: Some(crate::api::value::Points(20.0)),
                lock_aspect: None,
            }),
            fonts(),
        );
        assert!(matches!(out, super::super::Outcome::Unchanged(_)), "{out:?}");
        assert_eq!(size(&r.e), before);
        r
    };
    let mut r = check(super::scenarios::Run::open("FX-PLACEHOLDER.docx", "fixed"));
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    r.e.execute(
        Command::FillPlaceholder(crate::api::command::FillPlaceholder {
            key: Some("Signature.Recipient Sig".into()),
            image: crate::api::value::Bytes(data),
        }),
        fonts(),
    );
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert!(!again.document().fixed.is_empty(), "filled controls stay fixed");
    // An ordinary picture keeps its handles.
    let mut o = super::scenarios::Run::open("FX-OBJ.docx", "not fixed");
    assert!(o.e.document().fixed.is_empty());
    let st = crate::editing::inspect::inspect(&mut o.e, "geometry", true);
    assert!(!st["geometry"]["pictures"].as_array().is_none_or(|a| a.is_empty()));
    o.step(&json!({ "click": { "picture": 0 } }));
    assert!(matches!(o.e.view(0..1).selection, crate::editing::view::SelectionView::Object(_, true)));
}

#[test]
fn sdt_locked_picture_controls_cant_be_deleted() {
    // A picture control with w:lock sdtLocked ("Content control cannot be deleted"): Delete on it, a deletion or
    // typing over a selection holding it, and Cut leave the document as it was; without the lock, Delete takes it.
    use serde_json::json;
    let bytes = {
        use std::io::{Read, Write};
        let src = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-PLACEHOLDER.docx")).expect("fixture");
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            let mut data = Vec::new();
            f.read_to_end(&mut data).unwrap();
            let name = f.name().unwrap().to_string();
            if name == "word/document.xml" {
                data = String::from_utf8(data).unwrap().replace("<w:picture />", "<w:lock w:val=\"sdtLocked\"/><w:picture />").into_bytes();
            }
            out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            out.write_all(&data).unwrap();
        }
        out.finish().unwrap().into_inner()
    };
    let pictures = |e: &super::super::Editor| texts(e).iter().map(|t| t.matches('\u{FFFC}').count()).sum::<usize>();
    let select_picture = |r: &mut super::scenarios::Run| {
        let st = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
        let n = st["geometry"]["pictures"].as_array().map_or(0, |a| a.len());
        (0..n)
            .find(|&k| {
                r.step(&json!({ "click": { "picture": k } }));
                r.e.selected_placeholder().is_some()
            })
            .expect("the placeholder")
    };
    let mut r = super::scenarios::Run::from_bytes(&bytes, "undeletable");
    assert!(!r.e.document().undeletable.is_empty(), "read as undeletable");
    let before = texts(&r.e);
    let n = pictures(&r.e);
    let cases: Vec<(&str, Vec<serde_json::Value>)> = vec![
        ("Delete on it", vec![json!({ "key": "Delete" })]),
        ("Backspace on it", vec![json!({ "key": "Backspace" })]),
        ("Select All, Delete", vec![json!({"command": {"type": "selectAll"}}), json!({ "key": "Delete" })]),
        ("Select All, type", vec![json!({"command": {"type": "selectAll"}}), json!({ "type": "x" })]),
        ("Select All, Cut", vec![json!({"command": {"type": "selectAll"}}), json!({"command": {"type": "cut"}})]),
    ];
    for (name, steps) in cases {
        select_picture(&mut r);
        for s in &steps {
            r.step(s);
        }
        assert_eq!(texts(&r.e), before, "{name}");
    }
    assert_eq!(pictures(&r.e), n);
    // Other edits still go: typing elsewhere.
    r.step(&json!({ "click": { "para": 0, "offset": 0 } }));
    r.step(&json!({ "type": "Q" }));
    assert_ne!(texts(&r.e), before);
    // Without the lock, Delete takes it.
    let mut o = super::scenarios::Run::open("FX-PLACEHOLDER.docx", "deletable");
    let m = pictures(&o.e);
    select_picture(&mut o);
    o.step(&json!({ "key": "Delete" }));
    assert_eq!(pictures(&o.e), m - 1);
}

#[test]
fn alt_text_saves() {
    // Alt Text: the selected picture's description, one step, written into its wp:docPr and read back.
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-OBJ.docx", "alt-text");
    r.step(&json!({ "click": { "picture": 0 } }));
    let out = r.e.execute(Command::SetAltText(crate::api::command::AltText { text: "The board seal".into(), decorative: None }), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let alts = |d: &crate::docx::Document| -> Vec<String> {
        d.sections
            .iter()
            .flat_map(|s| s.blocks.iter())
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
            .flat_map(|p| p.inlines.iter())
            .filter_map(|i| if let crate::docx::Inline::Picture(p, _) = i { Some(p.alt.to_string()) } else { None })
            .collect()
    };
    assert!(alts(r.e.document()).contains(&"The board seal".to_string()));
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(alts(again.document()), alts(r.e.document()));
    assert_eq!(texts(&again), texts(&r.e));
    r.e.execute(Command::Undo, fonts());
    assert!(!alts(r.e.document()).contains(&"The board seal".to_string()));
}

#[test]
fn extend_mode_grows_and_extends() {
    // F8, then F8 again selects the word, the sentence, the paragraph, the document; arrows extend; Esc keeps
    // the selection and ends the mode; an edit ends it too.
    use serde_json::json;
    let mut e = super::super::Editor::open(&blank_with("The quick brown fox jumps. Over the lazy dog.\nSecond paragraph here."), fonts()).unwrap();
    let sel = |e: &mut super::super::Editor| e.selected_text();
    e.execute(crate::editing::tests::mv(crate::editing::commands::MoveBy::Story, crate::editing::commands::Dir::Back, false), fonts());
    for _ in 0..6 {
        e.execute(crate::editing::tests::mv(crate::editing::commands::MoveBy::Char, crate::editing::commands::Dir::Forward, false), fonts());
    }
    assert_eq!(e.execute(Command::SetSelectionMode(crate::api::command::SelectionMode::Extend), fonts()), super::super::Outcome::Moved);
    assert_eq!(sel(&mut e), "", "the first F8 only turns the mode on");
    e.execute(Command::SetSelectionMode(crate::api::command::SelectionMode::Extend), fonts());
    assert_eq!(sel(&mut e).trim_end(), "quick");
    e.execute(Command::SetSelectionMode(crate::api::command::SelectionMode::Extend), fonts());
    assert_eq!(sel(&mut e).trim_end(), "The quick brown fox jumps.");
    e.execute(Command::SetSelectionMode(crate::api::command::SelectionMode::Extend), fonts());
    assert!(sel(&mut e).starts_with("The quick brown fox jumps. Over the lazy dog."));
    e.execute(Command::SetSelectionMode(crate::api::command::SelectionMode::Extend), fonts());
    assert!(sel(&mut e).contains("Second paragraph here."), "the document");
    // Esc: the selection stays, the mode ends: an arrow collapses it again.
    e.execute(Command::Escape, fonts());
    assert!(!e.selection().is_collapsed());
    e.execute(crate::editing::tests::mv(crate::editing::commands::MoveBy::Char, crate::editing::commands::Dir::Back, false), fonts());
    assert!(e.selection().is_collapsed());
    // F8 then arrows extend from the caret; a typed character extends to its next occurrence (case-sensitive) and
    // the mode stays on.
    e.execute(Command::SetSelectionMode(crate::api::command::SelectionMode::Extend), fonts());
    for _ in 0..3 {
        e.execute(crate::editing::tests::mv(crate::editing::commands::MoveBy::Char, crate::editing::commands::Dir::Forward, false), fonts());
    }
    assert_eq!(sel(&mut e), "The");
    e.execute(Command::InsertText("b".into()), fonts());
    assert_eq!(sel(&mut e), "The quick b");
    e.execute(Command::InsertText("Q".into()), fonts());
    assert_eq!(sel(&mut e), "The quick b", "no capital Q after it");
    e.execute(crate::editing::tests::mv(crate::editing::commands::MoveBy::Char, crate::editing::commands::Dir::Forward, false), fonts());
    assert_eq!(sel(&mut e), "The quick br", "still extending");
    let _ = json!(null);
}

#[test]
fn block_selection_selects_copies_formats_and_deletes() {
    // Ctrl+Shift+F8, then arrows select a vertical block: each line's piece between the two x positions.
    use crate::editing::commands::{Dir, MoveBy};
    let text = "abcdefgh\nijklmnop\nqrstuvwx\nyz";
    let mut e = super::super::Editor::open(&super::blank_in(text, "Courier New"), fonts()).unwrap();
    let mv = |e: &mut super::super::Editor, by, dir| {
        e.execute(crate::editing::tests::mv(by, dir, false), fonts());
    };
    mv(&mut e, MoveBy::Story, Dir::Back);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    e.execute(Command::SetSelectionMode(crate::api::command::SelectionMode::Block), fonts());
    mv(&mut e, MoveBy::Line, Dir::Forward);
    mv(&mut e, MoveBy::Line, Dir::Forward);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    assert!(matches!(e.selection(), crate::editing::selection::Selection::Block { .. }), "{:?}", e.selection());
    assert_eq!(e.selected_text(), "cde\nklm\nstu");
    e.execute(Command::SetBold(true), fonts());
    let bold: Vec<String> = texts(&e)
        .iter()
        .enumerate()
        .map(|(k, _)| {
            let p = match &e.document().sections[0].blocks[k] {
                crate::docx::Block::Para(p) => p,
                _ => panic!(),
            };
            p.inlines
                .iter()
                .filter_map(|i| if let crate::docx::Inline::Text(t, r) = i { r.bold.then(|| t.clone()) } else { None })
                .collect::<String>()
        })
        .collect();
    assert_eq!(bold, ["cde", "klm", "stu", ""], "only the block is bold");
    let before = texts(&e);
    e.execute(crate::editing::tests::del(Dir::Forward, false), fonts());
    assert_eq!(texts(&e), ["abfgh", "ijnop", "qrvwx", "yz"]);
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), before);
    // Esc in block mode: the caret at the block's start.
    e.execute(Command::SetSelectionMode(crate::api::command::SelectionMode::Block), fonts());
    mv(&mut e, MoveBy::Line, Dir::Forward);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    assert!(matches!(e.selection(), crate::editing::selection::Selection::Block { .. }));
    e.execute(Command::Escape, fonts());
    assert!(e.selection().is_collapsed());
}

#[test]
fn go_back_returns_to_earlier_edits() {
    // Shift+F5 goes to the last edit location, again to the one before, round them; the locations
    // follow later edits.
    use crate::editing::commands::{Dir, MoveBy};
    let mut e = super::super::Editor::open(&blank_with("First paragraph.\nSecond paragraph.\nThird paragraph."), fonts()).unwrap();
    let mv = |e: &mut super::super::Editor, by, dir| {
        e.execute(crate::editing::tests::mv(by, dir, false), fonts());
    };
    let para_no = |e: &super::super::Editor| {
        let (p, u) = e.selection().focus().and_then(|p| p.para_offset()).unwrap();
        let k = e.document().sections[0].blocks.iter().position(|b| matches!(b, crate::docx::Block::Para(q) if q.id == p.0)).unwrap();
        (k, u.0)
    };
    // An edit at the end of the first paragraph, then one at the start of the third.
    mv(&mut e, MoveBy::Story, Dir::Back);
    mv(&mut e, MoveBy::LineEdge, Dir::Forward);
    e.execute(Command::InsertText("!".into()), fonts());
    mv(&mut e, MoveBy::Story, Dir::Forward);
    mv(&mut e, MoveBy::LineEdge, Dir::Back);
    e.execute(Command::InsertText("> ".into()), fonts());
    // Elsewhere: the second paragraph (Ctrl+Up to this one's start, then to the one before).
    mv(&mut e, MoveBy::Para, Dir::Back);
    mv(&mut e, MoveBy::Para, Dir::Back);
    assert_eq!(para_no(&e).0, 1);
    e.execute(Command::GoBack, fonts());
    assert_eq!(para_no(&e), (2, 2), "the last edit");
    e.execute(Command::GoBack, fonts());
    assert_eq!(para_no(&e), (0, 17), "the one before");
    e.execute(Command::GoBack, fonts());
    assert_eq!(para_no(&e), (1, 0), "then where the caret was");
    e.execute(Command::GoBack, fonts());
    assert_eq!(para_no(&e), (2, 2), "round again");
    // A later edit before the first location moves it along.
    mv(&mut e, MoveBy::Story, Dir::Back);
    e.execute(Command::InsertText("123".into()), fonts());
    mv(&mut e, MoveBy::Para, Dir::Forward);
    e.execute(Command::GoBack, fonts());
    assert_eq!(para_no(&e), (0, 3), "the newest edit");
    e.execute(Command::GoBack, fonts());
    assert_eq!(para_no(&e), (2, 2));
    e.execute(Command::GoBack, fonts());
    assert_eq!(para_no(&e), (0, 20), "mapped through the insertion before it");
}

#[test]
fn drag_and_drop_moves_and_copies_text() {
    // A press inside the selection and a release without moving collapses it there; a drag moves the
    // text (one undo step) and selects it; Ctrl+drag copies; a drop onto the selection changes nothing.
    use serde_json::json;
    let doc = blank_with("The quick brown fox.\nJumps over.");
    let select_quick = |r: &mut super::scenarios::Run| {
        r.step(&json!({ "click": { "para": 0, "offset": 4 } }));
        for _ in 0..5 {
            r.step(&json!({ "key": "ArrowRight", "mods": { "shift": true } }));
        }
        assert_eq!(r.e.selected_text(), "quick");
    };
    // A click inside: the caret goes there.
    let mut r = super::scenarios::Run::from_bytes(&doc, "dnd-click");
    select_quick(&mut r);
    r.step(&json!({ "click": { "para": 0, "offset": 6 } }));
    assert!(r.e.selection().is_collapsed());
    // A drag to the second paragraph's end: moved, selected there, one undo.
    let mut r = super::scenarios::Run::from_bytes(&doc, "dnd-move");
    select_quick(&mut r);
    let before = texts(&r.e);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 6 }, "to": { "para": 1, "offset": "end" } } }));
    let t = texts(&r.e);
    assert_eq!(t[0], "The brown fox.", "no doubled space left (smart cut)");
    assert!(t[1].contains("quick"), "{t:?}");
    assert_eq!(r.e.selected_text().trim(), "quick");
    r.e.execute(Command::Undo, fonts());
    assert_eq!(texts(&r.e), before, "one undo step");
    // Ctrl+drag copies.
    let mut r = super::scenarios::Run::from_bytes(&doc, "dnd-copy");
    select_quick(&mut r);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 6 }, "to": { "para": 1, "offset": 0 } }, "mods": { "ctrl": true } }));
    let t = texts(&r.e);
    assert!(t[0].contains("quick") && t[1].contains("quick"), "{t:?}");
    // Onto the selection itself: nothing changes.
    let mut r = super::scenarios::Run::from_bytes(&doc, "dnd-self");
    select_quick(&mut r);
    let before = texts(&r.e);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 5 }, "to": { "para": 0, "offset": 8 } } }));
    assert_eq!(texts(&r.e), before);
}

/// A table built by typing: Insert Table rows x cols, then each cell's text (row by row, Tab between).
fn typed_table(cells: &[&[&str]]) -> super::super::Editor {
    let mut e = super::super::Editor::open(&blank_with(""), fonts()).unwrap();
    e.execute(Command::InsertTable(crate::api::command::TableSize { rows: cells.len() as u32, columns: cells[0].len() as u32 }), fonts());
    for (r, row) in cells.iter().enumerate() {
        for (c, t) in row.iter().enumerate() {
            e.execute(Command::InsertText((*t).into()), fonts());
            if r + 1 < cells.len() || c + 1 < row.len() {
                e.execute(Command::MoveToCell(crate::api::value::Order::Next), fonts());
            }
        }
    }
    e
}

fn table_texts(e: &super::super::Editor) -> Vec<Vec<String>> {
    let t = e.document().sections[0].blocks.iter().find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).unwrap();
    t.rows
        .iter()
        .map(|r| {
            r.cells
                .iter()
                .map(|c| {
                    c.blocks
                        .iter()
                        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(crate::docx::fields::text(p)) } else { None })
                        .collect::<String>()
                })
                .collect()
        })
        .collect()
}

#[test]
fn rows_move_up_and_down() {
    // Alt+Shift+Up / Down move the caret's row; the caret goes with it; one undo each.
    use crate::editing::keymap::Mods;
    let mut e = typed_table(&[&["a1", "a2"], &["b1", "b2"], &["c1", "c2"]]);
    // The caret is in the last cell (c2): up one row.
    let up = e.key_command("ArrowUp", Mods { alt: true, shift: true, ..Mods::default() }).unwrap();
    e.execute(up, fonts());
    assert_eq!(table_texts(&e), [["a1", "a2"], ["c1", "c2"], ["b1", "b2"]]);
    e.execute(Command::MoveRows(crate::api::command::VerticalDirection::Up), fonts());
    assert_eq!(table_texts(&e), [["c1", "c2"], ["a1", "a2"], ["b1", "b2"]]);
    assert!(
        matches!(e.execute(Command::MoveRows(crate::api::command::VerticalDirection::Up), fonts()), super::super::Outcome::Unchanged(_)),
        "at the top"
    );
    assert!(matches!(e.selection(), Selection::Cells { anchor: crate::editing::selection::CellRef { row: 0, .. }, .. }), "the moved row is selected");
    e.execute(Command::Undo, fonts());
    assert_eq!(table_texts(&e), [["a1", "a2"], ["c1", "c2"], ["b1", "b2"]]);
    let saved = crate::docx::write::save(e.document()).unwrap();
    assert_eq!(table_texts(&super::super::Editor::open(&saved, fonts()).unwrap()), table_texts(&e), "saved in the new order");
}

#[test]
fn double_click_auto_fits_a_column() {
    // The column fits its content; the others keep their widths; the table's width changes.
    let mut e = typed_table(&[
        &["h1", "h2", "h3"],
        &["A longer piece of text in column one that wraps onto a second line here.", "short", "medium length text"],
        &["A longer piece of text in column one that wraps onto a second line here.", "short", "medium length text"],
    ]);
    let grid = |e: &super::super::Editor| {
        e.document().sections[0].blocks.iter().find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t.grid.clone()) } else { None }).unwrap()
    };
    let table = e
        .index()
        .para_ids()
        .find_map(|p| {
            e.index()
                .para_slot(p)
                .and_then(|s| if let crate::editing::index::Container::Cell { table, .. } = s.container { Some(table) } else { None })
        })
        .unwrap();
    let before = grid(&e);
    e.auto_fit_column(table, 1, fonts());
    let g = grid(&e);
    assert!(g[1] < before[1] / 2.0 && g[1] > 20.0, "'short' fits: {g:?}");
    assert_eq!((g[0], g[2]), (before[0], before[2]), "the others keep their widths");
    e.auto_fit_column(table, 0, fonts());
    let g2 = grid(&e);
    assert!(g2[0] > g[0], "the long text widens its column: {g2:?}");
    assert!(g2.iter().sum::<f64>() <= 468.0 + 12.0, "at most the text width and the padding: {g2:?}");
}

#[test]
fn ctrl_alt_5_and_tab_go_through_floats() {
    // Ctrl+Alt+5 selects the first floating object; Tab the next, Shift+Tab back; Esc to the text.
    use crate::editing::keymap::Mods;
    let mut e = open("FX-OBJ.docx");
    let five = e.key_command("Digit5", Mods { ctrl: true, alt: true, ..Mods::default() }).unwrap();
    e.execute(five, fonts());
    let first = e.selection();
    assert!(matches!(first, Selection::Object(crate::editing::selection::ObjectRef::Floating(_))), "{first:?}");
    let tab = e.key_command("Tab", Mods::default()).unwrap();
    e.execute(tab, fonts());
    let second = e.selection();
    assert!(matches!(second, Selection::Object(crate::editing::selection::ObjectRef::Floating(_))));
    let back = e.key_command("Tab", Mods { shift: true, ..Mods::default() }).unwrap();
    e.execute(back, fonts());
    assert_eq!(e.selection(), first);
    e.execute(Command::Escape, fonts());
    assert!(e.selection().is_collapsed(), "Esc returns to the text");
}

#[test]
fn the_caret_shows_after_the_document_gets_shorter_than_its_last_page() {
    // The caret was drawn on the last page; select all and type leaves one page. Its page search starts from the
    // page it was last on, which no longer exists (FUZZ-FX-TEXT-11-b: Page Up did nothing after drawn frames).
    let mut e = open("FX-TEXT.docx");
    e.execute(crate::editing::tests::mv(MoveBy::Story, Dir::Forward, false), fonts());
    e.complete(fonts());
    assert!(e.caret_view().expect("caret").page > 0, "FX-TEXT is longer than a page");
    e.execute(Command::SelectAll, fonts());
    e.execute(Command::InsertText("x".into()), fonts());
    e.complete(fonts());
    assert_eq!(e.snapshot().pages().len(), 1);
    assert!(e.caret_view().is_some(), "the caret shows");
}
