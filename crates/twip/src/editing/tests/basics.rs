//! Editing basics

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
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), before);
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(texts(&e), after);
}

#[test]
fn typing_elsewhere_in_the_paragraph_is_an_undo_step_of_its_own() {
    use super::super::position::Position;
    let mut e = open("FX-TEXT.docx");
    let para = super::para_id(&e, 1);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para, Unit(2))));
    type_text(&mut e, "abc");
    let typed = texts(&e);
    // A caret move that writes the selection itself, as Shift+F9 and extend mode do, keeps the typing run.
    e.selection = Selection::Caret(Position::text(StoryId::Body, para, Unit(0)));
    type_text(&mut e, "x");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), typed, "one undo takes back only the x");
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
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), split);
    e.execute(Command::Undo(Default::default()), fonts());
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
    let mut e = open("FX-TEXT.docx");
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
    e.execute(Command::Undo(Default::default()), fonts);
    let t = e.index().table(e.document(), crate::layout::TableId(inner)).expect("inner table");
    assert_eq!(t.rows.len(), rows);
}

#[test]
fn every_click_on_a_line_lands_in_its_paragraph() {
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
    let mut e = open("FX-TEXT.docx");
    let before = e.history().bytes();
    type_text(&mut e, &"abcdefghij".repeat(100));
    assert!(e.history().bytes() - before <= 1 << 20, "{} bytes", e.history().bytes() - before);
    assert_eq!(e.history().undo_depth(), 1, "one typing run is one undo step");
}

#[test]
fn added_rows_and_typing_in_them_save_and_reopen() {
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
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    for f in ["FX-NEST-3.docx", "FX-TBL-MERGED.docx", "FX-TBL-BASIC.docx", "FX-NEST-DEEP.docx", "FX-NEST-MIXED.docx", "FX-OBJ-CELL.docx"] {
        let mut e = open(f);
        e.complete(fonts());
        let stack = e.stack();
        let mut checked = 0;
        for (page, g) in e.page_geometries() {
            for l in g.lines.iter().filter(|l| !l.repeat && !l.chain.is_empty() && g.stories[l.story].story == StoryId::Body) {
                let x = match l.stops.get(1) {
                    Some(s) => (l.stops[0].x + s.x) / 2.0,
                    None => l.stops[0].x + 1.0,
                };
                let y = (l.top + l.bottom) / 2.0;
                let own = g.cells.iter().find(|c| c.chain == l.chain).expect("the line's cell");
                if x < own.rect[0] || x > own.rect[2] {
                    continue;
                }
                let (dx, dy) = stack.to_doc(page, x, y).expect("on the stack");
                for kind in [PointerKind::Down, PointerKind::Up] {
                    e.pointer_input(kind, dx, dy, 1, Mods::default(), fonts());
                }
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
    use serde_json::json;
    let tables = |e: &super::super::Editor| -> Vec<(Vec<f64>, Vec<Option<(f64, crate::docx::HeightRule)>>, Vec<Option<crate::docx::props::Width>>)> {
        fn walk(
            b: &[crate::docx::Block],
            out: &mut Vec<(Vec<f64>, Vec<Option<(f64, crate::docx::HeightRule)>>, Vec<Option<crate::docx::props::Width>>)>,
        ) {
            for b in b {
                if let crate::docx::Block::Table(t) = b {
                    out.push((
                        t.grid.clone(),
                        t.rows.iter().map(|r| r.height).collect(),
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
    assert!(edited[1].1[1].as_ref().is_some_and(|(_, rule)| *rule == crate::docx::HeightRule::AtLeast), "{:?}", edited[1].1);
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-TBL-BASIC.resize.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    let back = tables(&again);
    assert_eq!(back[1].0, edited[1].0);
    let h = |v: &Option<(f64, crate::docx::HeightRule)>| v.as_ref().map(|(h, r)| ((h * 20.0).round(), *r));
    assert_eq!(back[1].1.iter().map(h).collect::<Vec<_>>(), edited[1].1.iter().map(h).collect::<Vec<_>>());
    assert_eq!(back[1].2, edited[1].2);
    assert_eq!(texts(&again), texts(&r.e));
}

#[test]
fn a_reanchored_float_saves_in_its_new_paragraph() {
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
fn a_float_or_text_box_dragged_onto_the_next_page_lands_there() {
    use serde_json::json;
    for (fx, at, dy, want) in [("FX-OBJ.docx", "", 400.0, 1), ("FX-STORIES.docx", "top", 900.0, 1)] {
        let n = if fx == "FX-OBJ.docx" { 1 } else { 0 };
        let mut r = super::scenarios::Run::open(fx, "cross-page");
        r.step(&json!({ "drag": { "from": { "float": n, "at": at }, "to": { "float": n, "at": at, "dy": dy } } }));
        let g = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
        let f = g["geometry"]["floats"].as_array().unwrap().iter().find(|f| f["float"] == json!(n)).cloned().unwrap();
        let top = g["geometry"]["pages"][want][1].as_f64().unwrap();
        assert_eq!(f["page"], json!(want), "{fx}: {f}");
        assert!(f["rect"][1].as_f64().unwrap() > top, "{fx}: {f}");
    }
    // Held by its bottom edge, the box follows its centre, not the pointer: the pointer is over page 2, the box's
    // centre still over page 1.
    let mut r = super::scenarios::Run::open("FX-STORIES.docx", "cross-page-centre");
    r.step(&json!({ "drag": { "from": { "float": 0, "at": "bottom" }, "to": { "float": 0, "at": "bottom", "dy": 650.0 } } }));
    let g = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
    assert_eq!(g["geometry"]["floats"][0]["page"], json!(0), "{}", g["geometry"]["floats"]);
    // The showcase's banner, floated, crosses from the cover's section into the next one.
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/docx-feature-showcase.docx")).expect("fixture");
    let mut r = super::scenarios::Run::from_bytes(&bytes, "cross-section");
    r.step(&json!({ "click": { "picture": 0 } }));
    let out = r.e.execute(super::super::commands::Command::SetObjectWrap(crate::api::value::ObjectWrap::Square), fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    r.step(&json!({ "drag": { "from": { "float": 0 }, "to": { "float": 0, "dy": 700.0 } } }));
    let g = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
    let f = g["geometry"]["floats"].as_array().unwrap().iter().find(|f| f["float"] == json!(0)).cloned().unwrap();
    assert_eq!(f["page"], json!(1), "{f}");
}

#[test]
fn a_shape_selects_on_a_click_and_opens_on_a_double_click_and_a_text_box_opens_at_once() {
    use super::super::Selection;
    use super::super::position::StoryId;
    use serde_json::json;
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/docx-feature-showcase.docx")).expect("fixture");
    let mut r = super::scenarios::Run::from_bytes(&bytes, "shape-clicks");
    let g = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
    // One of the shapes with text on page 8.
    let draft = g["geometry"]["floats"].as_array().unwrap().iter().find(|f| f["page"] == json!(7)).expect("a shape")["float"].clone();
    r.step(&json!({ "click": { "float": draft } }));
    assert!(matches!(r.e.selection(), Selection::Object(_)), "a click selects the shape: {:?}", r.e.selection());
    let mut r = super::scenarios::Run::from_bytes(&bytes, "shape-clicks");
    r.step(&json!({ "click": { "float": draft }, "clicks": 2 }));
    let caret = r.e.selection().focus().expect("a caret");
    assert!(matches!(caret.story, StoryId::TextBox(_)) && r.e.selection().is_collapsed(), "a double-click opens its text: {caret:?}");
    r.step(&json!({ "click": { "float": draft } }));
    assert!(r.e.selection().is_collapsed(), "a click in the shape being edited stays in its text");
    // Each shape, Review's ellipse too (its text area is narrow, its frame inset by its outline), is selected by a
    // click by its text's start.
    for f in g["geometry"]["floats"].as_array().unwrap().iter().filter(|f| f["page"] == json!(7)).take(3) {
        let v: Vec<f64> = f["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let mut r = super::scenarios::Run::from_bytes(&bytes, "shape-clicks");
        let (x, y) = (v[0] + (v[2] - v[0]) * 0.3, (v[1] + v[3]) / 2.0);
        r.send(json!({ "type": "pointer", "kind": "down", "x": x, "y": y, "clicks": 1, "mods": {} }));
        r.send(json!({ "type": "pointer", "kind": "up", "x": x, "y": y, "clicks": 1 }));
        assert!(matches!(r.e.selection(), Selection::Object(_)), "{v:?}: {:?}", r.e.selection());
        let pages = r.e.snapshot().pages().len();
        let view = r.e.view(0..pages);
        assert!(
            matches!(&view.selection, super::super::view::SelectionView::Object(o, true) if !o.is_empty()),
            "{v:?}: its outline and handles draw: {:?}",
            view.selection
        );
    }
    let mut r = super::scenarios::Run::open("FX-STORIES.docx", "box-clicks");
    r.step(&json!({ "click": { "float": 0 } }));
    assert!(r.e.selection().focus().is_some_and(|p| matches!(p.story, StoryId::TextBox(_))), "a text box opens at once");
}

#[test]
fn a_border_moved_in_selected_rows_saves() {
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
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(4))));
    e.execute(crate::editing::tests::del(super::super::commands::Dir::Forward, false), fonts());
    assert_eq!(note_texts(&e), before);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(note_texts(&again), before);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(note_texts(&e), edited);
}

#[test]
fn notes_inserted_in_a_document_without_notes_save() {
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
    use crate::layout::document::{FAIL_VERIFY, HF_ALTERNATE};
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
        e.execute(Command::Undo(Default::default()), fonts());
        assert_eq!(texts(&e), before);
        HF_ALTERNATE.set(0.0);
        FAIL_VERIFY.set(false);
    }
}

fn open_large() -> super::super::Editor {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/big.docx")).expect("big.docx");
    super::super::Editor::open(&bytes, fonts()).expect("opens")
}

#[test]
fn down_right_after_enter_lands_where_the_finished_layout_puts_it() {
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
        let (p, u) = e.selection().focus().and_then(|f| f.para_offset()).expect("a caret");
        ((0..texts(&e).len()).position(|k| super::super::tests::para_id(&e, k) == p), u)
    };
    assert_eq!(run(false), run(true));
}

#[test]
fn a_late_selection_keeps_its_characters_through_an_early_reflow() {
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
    assert_eq!((now[0].wrap, now[0].locked), (crate::docx::FloatWrap::Tight, !was[0].locked));
    assert_eq!((now[1].wrap, now[1].behind), (crate::docx::FloatWrap::None, true));
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
    for _ in 0..4 {
        e.execute(Command::Undo(Default::default()), fonts());
    }
    let undone = body_floats(&e);
    assert!(undone.iter().zip(&was).all(|(a, b)| a.wrap == b.wrap && a.behind == b.behind && a.locked == b.locked));
}

#[test]
fn an_inline_picture_resizes_by_its_handle_and_saves() {
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

/// A picture control around a floating picture: reported, sized and given a placeholder picture by `setControl`,
/// resized and moved by the mouse, and saved with its control.
#[test]
fn a_floating_picture_control() {
    use serde_json::json;
    use std::io::{Read, Write};
    let src = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-services.docx")).expect("fixture");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        let name = f.name().unwrap().to_string();
        if name == "word/document.xml" {
            let x = String::from_utf8(data).unwrap();
            let (a, b) = (x.find("<w:r><w:drawing><wp:anchor").unwrap(), x.find("</wp:anchor></w:drawing></w:r>").unwrap() + 30);
            let sdt = r#"<w:sdt><w:sdtPr><w:tag w:val="Photo"/><w:id w:val="77"/><w:showingPlcHdr/><w:picture/></w:sdtPr><w:sdtContent>"#;
            data = format!("{}{sdt}{}</w:sdtContent></w:sdt>{}", &x[..a], &x[a..b], &x[b..]).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    let bytes = out.finish().unwrap().into_inner();
    let control = |e: &super::super::Editor| {
        let all = crate::editing::elements::enumerate(e.document(), e.index(), &Default::default());
        let c = all.into_iter().find(|x| x.props.as_ref().is_some_and(|p| p.tag.as_deref() == Some("Photo"))).expect("the control");
        let crate::docx::ElementId::Control(id) = c.id else { panic!("a control") };
        (id, e.picture_control_size(id).map(|s| (s.width.0, s.height.0)))
    };
    let mut r = super::scenarios::Run::from_bytes(&bytes, "float-control");
    let (c, size) = control(&r.e);
    assert_eq!(size, Some((72.0, 54.0)), "the float's size");
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%"><rect width="100%" height="100%" fill="#3F51B5"/></svg>"##;
    let set = json!({
        "id": crate::editing::elements::id_str(crate::docx::ElementId::Control(c)), "options": { "width": 144, "height": 108 },
        "placeholderImage": { "data": crate::api::value::base64(svg.as_bytes()), "mime": "image/svg+xml" },
    });
    let out = r.e.execute(Command::SetControl(serde_json::from_value(set).unwrap()), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let f = body_floats(&r.e).remove(0);
    let image = |e: &super::super::Editor, f: &crate::docx::Float| e.document().images[f.image.unwrap()].clone();
    assert_eq!(((f.w, f.h), image(&r.e, &f)), ((144.0, 108.0), svg.as_bytes().to_vec()));

    r.e.execute(Command::SetMode(crate::api::state::Mode::Design), fonts());
    r.step(&json!({ "click": { "float": 0 } }));
    r.step(&json!({ "drag": { "from": { "float": 0, "at": "bottom-right" }, "to": { "float": 0, "at": "bottom-right", "dx": 36.0 } } }));
    let g = body_floats(&r.e).remove(0);
    assert!(g.w > 170.0 && (g.w / g.h - 4.0 / 3.0).abs() < 0.02, "resized by its corner: {}×{}", g.w, g.h);
    r.step(&json!({ "drag": { "from": { "float": 0 }, "to": { "float": 0, "dx": -60.0 } } }));
    let m = body_floats(&r.e).remove(0);
    assert!((m.h_offset - g.h_offset).abs() > 30.0, "moved: {} -> {}", g.h_offset, m.h_offset);
    assert_eq!((control(&r.e).0, image(&r.e, &m)), (c, svg.as_bytes().to_vec()), "the control and its picture stay");

    let saved = crate::docx::write::save(r.e.document()).expect("saves");
    let mut xml = String::new();
    zip::ZipArchive::new(std::io::Cursor::new(saved.clone())).unwrap().by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    let anchor = &xml[xml.find("<wp:anchor").unwrap()..xml.find("</wp:anchor>").unwrap()];
    assert!(anchor.contains("asvg:svgBlip"), "{anchor}");
    let again = super::super::Editor::open(&saved, fonts()).expect("reopens");
    let back = body_floats(&again).remove(0);
    assert_eq!((control(&again).1, image(&again, &back)), (Some((m.w, m.h)), svg.as_bytes().to_vec()));
}

#[test]
fn reset_size_gives_a_picture_its_own_size() {
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
            ..Default::default()
        }),
        fonts(),
    );
    assert_eq!((pic(&e).0, pic(&e).1), (20.0, 10.0));
    e.execute(Command::ResetObjectSize, fonts());
    let image = pic(&e).2;
    let (w, h, dx, dy) = crate::docx::image::image_info(&e.document().images[image.expect("an image")]).expect("decodes");
    let want = ((w * 72.0 / dx * 20.0).round() / 20.0, (h * 72.0 / dy * 20.0).round() / 20.0);
    assert_eq!((pic(&e).0, pic(&e).1), want);
}

fn object_box(e: &mut super::super::Editor) -> [f64; 4] {
    while e.idle(fonts()) {}
    match e.view(0..3).selection {
        super::super::view::SelectionView::Object(r, _) => r[0].rect,
        s => panic!("no object outline: {s:?}"),
    }
}

#[test]
fn an_inline_picture_floats_where_it_is_and_goes_back_in_line() {
    use crate::api::value::ObjectWrap;
    use crate::editing::selection::ObjectRef;
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-OBJ-LOCK.docx", "inline-to-float");
    let (pictures, floats) = (body_pictures(&r.e).len(), body_floats(&r.e).len());
    r.step(&json!({ "click": { "picture": 0 } }));
    let was = object_box(&mut r.e);
    let out = r.e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert!(matches!(r.e.selection(), Selection::Object(ObjectRef::Floating(_))), "{:?}", r.e.selection());
    assert_eq!((body_pictures(&r.e).len(), body_floats(&r.e).len()), (pictures - 1, floats + 1));
    let now = object_box(&mut r.e);
    // Off its paragraph's first line, it goes the quarter point below the paragraph's top that Word's conversion does.
    assert!(was.iter().zip(&now).all(|(a, b)| (a - b).abs() < 0.3), "stays where it was: {was:?} -> {now:?}");
    let f = body_floats(&r.e).into_iter().find(|f| f.src.is_none()).expect("the new float");
    assert_eq!(
        (f.wrap, f.h_relative.as_str(), f.v_relative.as_str(), f.image.is_some()),
        (crate::docx::FloatWrap::Square, "column", "paragraph", true)
    );
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-OBJ.inline-to-float.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!((body_pictures(&again).len(), body_floats(&again).len()), (pictures - 1, floats + 1));
    assert_eq!(texts(&again), texts(&r.e));
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((body_pictures(&r.e).len(), body_floats(&r.e).len()), (pictures, floats));
    r.e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!((body_pictures(&r.e).len(), body_floats(&r.e).len()), (pictures - 1, floats + 1));
    r.e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(crate::layout::FloatId(f.id))));
    assert!(matches!(r.e.execute(Command::SetObjectWrap(ObjectWrap::Inline), fonts()), super::super::Outcome::Changed(_)));
    assert!(matches!(r.e.selection(), Selection::Object(ObjectRef::Inline { .. })), "{:?}", r.e.selection());
    assert_eq!((body_pictures(&r.e).len(), body_floats(&r.e).len()), (pictures, floats));
    let back = object_box(&mut r.e);
    assert!(((back[2] - back[0]) - (was[2] - was[0])).abs() < 0.01 && ((back[3] - back[1]) - (was[3] - was[1])).abs() < 0.01);
    r.e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(crate::layout::FloatId(body_floats(&r.e)[0].id))));
    let fl = body_floats(&r.e)[0].clone();
    if fl.text_box.is_some() {
        assert!(matches!(r.e.execute(Command::SetObjectWrap(ObjectWrap::Inline), fonts()), super::super::Outcome::Refused(_)));
    }
}

#[test]
fn an_inline_picture_drags_to_a_new_place_in_the_text() {
    use crate::editing::selection::ObjectRef;
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-OBJ-LOCK.docx", "inline-drag");
    let pictures = body_pictures(&r.e).len();
    r.step(&json!({ "click": { "picture": 0 } }));
    let Selection::Object(ObjectRef::Inline { para, offset }) = r.e.selection() else { panic!("{:?}", r.e.selection()) };
    assert_eq!(offset, Unit(0));
    let text = |e: &super::super::Editor| texts(e);
    let before = text(&r.e);
    r.step(&json!({ "drag": { "from": { "picture": 0 }, "to": { "para": 1, "offset": 20 } } }));
    let Selection::Object(ObjectRef::Inline { para: p2, offset: o2 }) = r.e.selection() else { panic!("{:?}", r.e.selection()) };
    assert_eq!(p2, para);
    assert!(o2.0 > 10, "moved along the line: {o2:?}");
    let at = |e: &super::super::Editor, u: Unit| {
        matches!(e.index().para(e.document(), para).and_then(|p| super::super::stories::inline_at(p, u)), Some(crate::docx::Inline::Picture(..)))
    };
    assert!(at(&r.e, o2) && !at(&r.e, Unit(0)));
    assert_eq!(body_pictures(&r.e).len(), pictures);
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert!(at(&r.e, Unit(0)));
    assert_eq!(text(&r.e), before);
    r.step(&json!({ "click": { "picture": 0 } }));
    r.step(&json!({ "drag": { "from": { "picture": 0 }, "to": { "para": 1, "offset": 20 } }, "mods": { "ctrl": true } }));
    assert_eq!(body_pictures(&r.e).len(), pictures + 1);
}

#[test]
fn an_object_drag_shows_a_ghost_and_the_anchor_it_would_take() {
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-OBJ.docx", "ghost");
    r.e.live_layout = false;
    r.step(&json!({ "click": { "float": 1 } }));
    let was = object_box(&mut r.e);
    let anchor = r.e.view(0..3).anchor.expect("an anchor shows");
    assert!(matches!(r.e.view(0..3).selection, super::super::view::SelectionView::Object(_, true)), "handles after a click");
    r.step(&json!({ "press": { "float": 1 } }));
    assert!(matches!(r.e.view(0..3).selection, super::super::view::SelectionView::Object(_, true)), "handles on a press");
    r.step(&json!({ "move_to": { "float": 1, "dy": -150 }, "mods": { "alt": true } }));
    let v = r.e.view(0..3);
    let g = v.ghost.expect("a ghost");
    assert!(g.from.iter().zip(&was).all(|(a, b)| (a - b).abs() < 0.01), "{g:?}");
    assert!((g.to[1] - (was[1] - 150.0)).abs() < 0.5 && (g.to[0] - was[0]).abs() < 0.5, "{g:?}");
    assert!(g.float.is_some());
    assert!(matches!(v.selection, super::super::view::SelectionView::Object(_, false)), "no handles mid-drag");
    assert!(v.anchor.expect("an anchor shows").1[1] < anchor.1[1] - 50.0, "the anchor follows: {:?} -> {:?}", anchor, v.anchor);
    r.e.pointer_input(
        super::super::input::PointerKind::Move,
        (g.to[0] + g.to[2]) / 2.0 + 40.0,
        (g.from[1] + g.from[3]) / 2.0 - 150.0 + 10.0,
        0,
        super::super::keymap::Mods { shift: true, ..Default::default() },
        fonts(),
    );
    let g = r.e.view(0..3).ghost.expect("a ghost");
    assert!((g.to[1] - g.from[1]).abs() > 100.0 && (g.to[0] - g.from[0]).abs() < 0.01, "kept vertical: {g:?}");
}

#[test]
fn live_layout_lays_the_dragged_float_out_where_the_pointer_is() {
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-OBJ.docx", "live");
    let was = body_floats(&r.e);
    let id = was[1].id;
    let drawn = |e: &mut super::super::Editor| {
        while e.idle(fonts()) {}
        let n = body_floats(e).iter().position(|f| f.id == id);
        let g = crate::editing::inspect::inspect(e, "geometry", true)["geometry"].clone();
        let f = g["floats"].as_array().unwrap().iter().find(|f| f["float"] == json!(n)).unwrap()["rect"].clone();
        (f[1].as_f64().unwrap(), g["lines"].clone())
    };
    let undo = |e: &mut super::super::Editor| crate::editing::inspect::inspect(e, "undo", true)["undo"]["undo_depth"].clone();
    r.step(&json!({ "click": { "float": 1 } }));
    let (top, lines) = drawn(&mut r.e);
    let depth = undo(&mut r.e);
    r.step(&json!({ "press": { "float": 1 } }));
    r.step(&json!({ "move_to": { "float": 1, "dy": -150 }, "mods": { "alt": true } }));
    let (mid, mid_lines) = drawn(&mut r.e);
    assert!((mid - (top - 150.0)).abs() < 0.5, "laid out where the pointer is: {top} -> {mid}");
    assert_ne!(mid_lines, lines, "the text wraps around it there");
    assert!(r.e.view(0..3).ghost.is_none());
    assert_eq!(body_floats(&r.e), was, "the document is unchanged until the release");
    assert_eq!(undo(&mut r.e), depth);
    r.step(&json!({ "release": { "float": 1 }, "mods": { "alt": true } }));
    assert_ne!(body_floats(&r.e), was);
    let (after, after_lines) = drawn(&mut r.e);
    let differ: Vec<_> = after_lines.as_array().unwrap().iter().zip(mid_lines.as_array().unwrap()).filter(|(a, b)| a != b).take(2).collect();
    assert!((after - mid).abs() < 0.5 && after_lines == mid_lines, "the release lays out as the preview did: {after} vs {mid}, {differ:?}");
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(body_floats(&r.e), was, "one undo step");
    r.step(&json!({ "click": { "float": 1 } }));
    r.step(&json!({ "press": { "float": 1 } }));
    r.step(&json!({ "move_to": { "float": 1, "dy": -150 } }));
    r.e.execute(Command::Escape, fonts());
    assert_eq!(body_floats(&r.e), was);
    let (back, back_lines) = drawn(&mut r.e);
    assert!((back - top).abs() < 0.01 && back_lines == lines, "laid out as before the drag");
}

#[test]
fn escape_from_select_objects_ends_an_object_drag() {
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-OBJ.docx", "select-objects-escape");
    r.e.live_layout = false;
    let was = body_floats(&r.e);
    r.step(&json!({ "click": { "float": 1 } }));
    r.e.set_select_objects(true);
    r.step(&json!({ "press": { "float": 1 } }));
    r.step(&json!({ "move_to": { "float": 1, "dy": -150 } }));
    assert!(r.e.view(0..3).ghost.is_some(), "mid-drag");
    r.e.execute(Command::Escape, fonts());
    assert!(!r.e.selecting_objects() && r.e.view(0..3).ghost.is_none(), "the drag ended with the mode");
    r.step(&json!({ "release": { "float": 1 } }));
    assert_eq!(body_floats(&r.e), was, "the release moves nothing");
}

#[test]
fn a_live_drags_outline_stays_on_the_float_where_layout_keeps_it() {
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-OBJ.docx", "live-clamp");
    let f = crate::layout::FloatId(body_floats(&r.e)[1].id);
    r.step(&json!({ "click": { "float": 1 } }));
    let start = object_box(&mut r.e);
    r.step(&json!({ "press": { "float": 1 } }));
    r.step(&json!({ "move_to": { "float": 1, "dy": -1000 } }));
    let outline = object_box(&mut r.e);
    let laid = r.e.page_geometries().into_iter().find_map(|(_, g)| g.stories.iter().find(|s| s.float == Some(f)).map(|s| s.rect)).expect("drawn");
    assert!(laid[1] > start[1] - 1000.0 + 100.0, "layout keeps it on the page: {laid:?}");
    assert!(outline.iter().zip(&laid).all(|(a, b)| (a - b).abs() < 0.01), "the outline is on the float: {outline:?} vs {laid:?}");
}

#[test]
fn position_commands_place_the_float_and_the_state_reports_it() {
    use crate::api::command::{ObjectPosition, ObjectWrapOptions};
    use crate::api::value::*;
    use crate::editing::selection::ObjectRef;
    let mut e = open("FX-OBJ.docx");
    let f = crate::layout::FloatId(body_floats(&e)[1].id);
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(f)));
    let before = object_box(&mut e);
    let out = e.execute(
        Command::SetObjectPosition(ObjectPosition {
            horizontal: Some(HorizontalPosition {
                relative_to: HorizontalReference::Margin,
                align: Some(ObjectHorizontalAlignment::Right),
                offset: None,
                percent: None,
            }),
            ..Default::default()
        }),
        fonts(),
    );
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let now = object_box(&mut e);
    let sec = &e.document().sections[0];
    assert!((now[2] - (sec.page_w - sec.right)).abs() < 0.1, "right of the margin: {now:?}");
    assert!((now[1] - before[1]).abs() < 0.1, "vertically where it was");
    e.execute(
        Command::SetObjectPosition(ObjectPosition {
            vertical: Some(VerticalPosition { relative_to: VerticalReference::Page, align: None, offset: None, percent: None }),
            ..Default::default()
        }),
        fonts(),
    );
    let fixed = object_box(&mut e);
    assert!(fixed.iter().zip(&now).all(|(a, b)| (a - b).abs() < 0.1), "stays put: {now:?} -> {fixed:?}");
    let p = e.object_selection().and_then(|o| o.placement).expect("a placement");
    assert_eq!((p.horizontal.relative_to, p.horizontal.align), (HorizontalReference::Margin, Some(ObjectHorizontalAlignment::Right)));
    assert_eq!(p.vertical.relative_to, VerticalReference::Page);
    assert!((p.vertical.offset.expect("an offset").0 - fixed[1]).abs() < 0.1, "{p:?} at {fixed:?}");
    e.execute(
        Command::SetObjectWrapOptions(ObjectWrapOptions { side: Some(WrapSide::Largest), left: Some(Points(18.0)), ..Default::default() }),
        fonts(),
    );
    let p = e.object_selection().and_then(|o| o.placement).expect("a placement");
    assert_eq!((p.wrap_side, p.distance.left), (WrapSide::Largest, Points(18.0)));
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    let (a, b) = (&body_floats(&again)[1], &body_floats(&e)[1]);
    assert_eq!(
        (&a.h_relative, &a.h_align, &a.v_relative, &a.wrap_text, a.dist[2]),
        (&b.h_relative, &b.h_align, &b.v_relative, &b.wrap_text, b.dist[2])
    );
    assert!((a.v_offset - b.v_offset).abs() < 0.06);
    for _ in 0..3 {
        e.execute(Command::Undo(Default::default()), fonts());
    }
    let back = object_box(&mut e);
    assert!(back.iter().zip(&before).all(|(a, b)| (a - b).abs() < 0.1), "{before:?} -> {back:?}");
    let (para, offset) = e
        .index()
        .para_ids()
        .find_map(|id| {
            let p = e.index().para(e.document(), id)?;
            matches!(p.inlines.first(), Some(crate::docx::Inline::Picture(..))).then_some((id, Unit(0)))
        })
        .expect("an inline picture");
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Inline { para, offset }));
    assert!(e.object_selection().is_some_and(|o| o.placement.is_none()));
    assert!(!matches!(e.execute(Command::SetObjectPosition(ObjectPosition::default()), fonts()), super::super::Outcome::Changed(_)));
}

#[test]
fn an_inserted_picture_saves_with_its_image() {
    use crate::editing::selection::ObjectRef;
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut e = open("FX-STORIES.docx");
    let out =
        e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data.clone()), alt_text: None }), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert!(matches!(e.selection(), Selection::Object(ObjectRef::Inline { .. })), "{:?}", e.selection());
    let header = e.document().stories.keys().filter(|n| n.contains("header")).min().cloned().expect("a header");
    let story = e.index().header_footer(&header).expect("its story");
    let para =
        e.index().para_ids().find(|id| e.index().para_slot(*id).is_some_and(|s| e.index().story(s.container) == story)).expect("a header paragraph");
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(story, para, Unit(0))));
    e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data.clone()), alt_text: None }), fonts());
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
    e.execute(Command::Undo(Default::default()), fonts());
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(pictures(&e).is_empty());
}

#[test]
fn an_inserted_jpeg_xr_picture_saves_as_a_wdp_part() {
    let data = include_bytes!("../../../../twip-graphics/tests/images/alpha.jxr").to_vec();
    let mut e = super::super::Editor::open(&super::blank_with("Text"), fonts()).expect("opens");
    let out =
        e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data.clone()), alt_text: None }), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let types = crate::docx::package::read(&bytes, &[], "[Content_Types].xml").expect("reads").expect("content types");
    assert!(types.contains(r#"Extension="wdp" ContentType="image/vnd.ms-photo""#), "{types}");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert!(again.document().images.contains(&data));
}

#[test]
fn an_inserted_text_box_takes_typing_and_saves() {
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
    e.execute(Command::Undo(Default::default()), fonts());
    e.execute(Command::Undo(Default::default()), fonts());
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

fn table_formatting(e: &super::super::Editor) -> Vec<String> {
    fn walk(blocks: &[crate::docx::Block], out: &mut Vec<String>) {
        for b in blocks {
            if let crate::docx::Block::Table(t) = b {
                out.push(format!("table {:?} {:?} {:?} {:?} {:?} grid {:?}", t.style_id, t.look, t.width, t.jc, (t.ind * 20.0).round(), t.grid));
                for r in &t.rows {
                    out.push(format!("  row {:?} cant_split {}", r.height, r.cant_split));
                    for c in &r.cells {
                        out.push(format!(
                            "    cell {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
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
    let t =
        r.e.document().sections[0].blocks.iter().filter_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).nth(1).unwrap();
    let top = t.rows.iter().position(|row| row.height.as_ref().is_some_and(|h| h.1 == crate::docx::HeightRule::Exact)).expect("a row made exact");
    if top > 0 {
        let above = &t.rows[top - 1].cells[1];
        assert_eq!(above.bottom.as_ref().map(|b| &*b.style), Some("double"), "{above:?}");
    }
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-TBL-BASIC.properties.docx"), &bytes).expect("writes");
    }
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    same_lines(&table_formatting(&again), &edited);
    assert_eq!(texts(&again), texts(&r.e));
    for _ in 0..steps.len() {
        r.e.execute(Command::Undo(Default::default()), fonts());
    }
    same_lines(&table_formatting(&r.e), &before);
}

#[test]
fn a_table_style_and_its_options_restyle_the_cells() {
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
    let mut e = open("FX-TEXT.docx");
    assert!(e.document().stories.is_empty());
    assert!(matches!(e.execute(Command::EditHeader, fonts()), super::super::Outcome::Changed(_)));
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(StoryId::HeaderFooter(_))), "{:?}", e.selection());
    type_text(&mut e, "New header");
    e.execute(Command::CloseHeaderFooter, fonts());
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(StoryId::Body)));
    e.execute(Command::EditFooter, fonts());
    type_text(&mut e, "New footer");
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
    for _ in 0..4 {
        e.execute(Command::Undo(Default::default()), fonts());
    }
    assert!(e.document().stories.is_empty(), "{:?}", story_texts(&e));
}

#[test]
fn a_double_click_in_an_empty_header_area_makes_the_header() {
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
fn a_picture_control_activates_and_fills() {
    use serde_json::json;
    let mut r = super::scenarios::Run::open("FX-PLACEHOLDER.docx", "placeholder");
    let st = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
    let pics = st["geometry"]["pictures"].as_array().cloned().unwrap_or_default();
    let k = (0..pics.len())
        .find(|&k| {
            r.step(&json!({ "click": { "picture": k } }));
            r.e.selected_picture_unfilled()
        })
        .expect("a drawn unfilled picture control");
    let c = r.e.selected_picture_control().expect("its control");
    let pt = pics[k]["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>();
    let (x, y) = ((pt[0] + pt[2]) / 2.0, (pt[1] + pt[3]) / 2.0);
    let out = r.e.apply_json(&json!({ "type": "pointer", "kind": "down", "x": x, "y": y, "clicks": 1, "seq": 1000 }).to_string(), fonts());
    assert!(out.contains("picture_control_activated"), "{out}");
    r.e.apply_json(&json!({ "type": "pointer", "kind": "up", "x": x, "y": y, "seq": 1001 }).to_string(), fonts());
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let id = crate::api::value::ElementId(crate::editing::elements::id_str(crate::docx::ElementId::Control(c)));
    let out = r.e.execute(
        Command::FillControl(crate::api::command::FillControl {
            id,
            text: None,
            image: Some(crate::api::value::Bytes(data.clone())),
            docx: None,
            building_block: None,
            date: None,
        }),
        fonts(),
    );
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert!(!r.e.selected_picture_unfilled(), "filled: no longer showing its placeholder");
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    let xml = {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).unwrap();
        let mut s = String::new();
        z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
        s
    };
    assert!(xml.contains("Placeholder:Signature.Recipient Sig") && xml.contains("w:sdt"), "the control stays");
    assert!(!xml.contains("showingPlcHdr"), "no longer showing its placeholder");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert!(again.document().images.contains(&data), "the filled image is saved");
    assert!(again.document().controls.values().all(|k| !k.props.showing_placeholder), "read back as filled");
    assert_eq!(texts(&again), texts(&r.e));
}

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
            let want = if tag == "name" && line.starts_with("copy the name line") { 2 } else { 1 };
            assert_eq!(line.matches(&format!("(\"{tag}\"")).count(), want, "control {tag} lost or doubled: {line}");
        }
    }
}

#[test]
fn sdt_locked_picture_controls_cant_be_deleted() {
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
                r.e.selected_picture_control().is_some()
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
    r.step(&json!({ "click": { "para": 0, "offset": 0 } }));
    r.step(&json!({ "type": "Q" }));
    assert_ne!(texts(&r.e), before);
    let mut o = super::scenarios::Run::open("FX-PLACEHOLDER.docx", "deletable");
    let m = pictures(&o.e);
    select_picture(&mut o);
    o.step(&json!({ "key": "Delete" }));
    assert_eq!(pictures(&o.e), m - 1);
}

#[test]
fn alt_text_saves() {
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
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert!(!alts(r.e.document()).contains(&"The board seal".to_string()));
}

#[test]
fn decorative_pictures_read_set_and_save() {
    use serde_json::json;
    let pictures = |d: &crate::docx::Document| -> Vec<(String, bool)> {
        d.sections
            .iter()
            .flat_map(|s| s.blocks.iter())
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
            .flat_map(|p| p.inlines.iter())
            .filter_map(|i| match i {
                crate::docx::Inline::Picture(p, _) => Some((p.alt.to_string(), p.shape.as_ref().is_some_and(|s| s.nv.decorative))),
                _ => None,
            })
            .collect()
    };
    let mut r = super::scenarios::Run::open("FX-A11Y-STRUCTURE.docx", "decorative");
    assert_eq!(pictures(r.e.document()), [("Board seal".to_string(), false), (String::new(), true)], "as read");
    r.step(&json!({ "click": { "picture": 0 } }));
    let out = r.e.execute(Command::SetAltText(crate::api::command::AltText { text: "ignored".into(), decorative: Some(true) }), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert_eq!(pictures(r.e.document())[0], (String::new(), true), "decorative: no alt text");
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(pictures(again.document()), [(String::new(), true), (String::new(), true)], "both decorative after a save");
    let mut r = super::scenarios::Run::open("FX-A11Y-STRUCTURE.docx", "decorative-off");
    r.step(&json!({ "click": { "picture": 1 } }));
    r.e.execute(Command::SetAltText(crate::api::command::AltText { text: "Divider".into(), decorative: Some(false) }), fonts());
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(pictures(again.document())[1], ("Divider".to_string(), false));
}

#[test]
fn extend_mode_grows_and_extends() {
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
    e.execute(Command::Escape, fonts());
    assert!(!e.selection().is_collapsed());
    e.execute(crate::editing::tests::mv(crate::editing::commands::MoveBy::Char, crate::editing::commands::Dir::Back, false), fonts());
    assert!(e.selection().is_collapsed());
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
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), before);
    e.execute(Command::SetSelectionMode(crate::api::command::SelectionMode::Block), fonts());
    mv(&mut e, MoveBy::Line, Dir::Forward);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    assert!(matches!(e.selection(), crate::editing::selection::Selection::Block { .. }));
    e.execute(Command::Escape, fonts());
    assert!(e.selection().is_collapsed());
}

#[test]
fn go_back_returns_to_earlier_edits() {
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
    mv(&mut e, MoveBy::Story, Dir::Back);
    mv(&mut e, MoveBy::LineEdge, Dir::Forward);
    e.execute(Command::InsertText("!".into()), fonts());
    mv(&mut e, MoveBy::Story, Dir::Forward);
    mv(&mut e, MoveBy::LineEdge, Dir::Back);
    e.execute(Command::InsertText("> ".into()), fonts());
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
    use serde_json::json;
    let doc = blank_with("The quick brown fox.\nJumps over.");
    let select_quick = |r: &mut super::scenarios::Run| {
        r.step(&json!({ "click": { "para": 0, "offset": 4 } }));
        for _ in 0..5 {
            r.step(&json!({ "key": "ArrowRight", "mods": { "shift": true } }));
        }
        assert_eq!(r.e.selected_text(), "quick");
    };
    let mut r = super::scenarios::Run::from_bytes(&doc, "dnd-click");
    select_quick(&mut r);
    r.step(&json!({ "click": { "para": 0, "offset": 6 } }));
    assert!(r.e.selection().is_collapsed());
    let mut r = super::scenarios::Run::from_bytes(&doc, "dnd-move");
    select_quick(&mut r);
    let before = texts(&r.e);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 6 }, "to": { "para": 1, "offset": "end" } } }));
    let t = texts(&r.e);
    assert_eq!(t[0], "The brown fox.", "no doubled space left (smart cut)");
    assert!(t[1].contains("quick"), "{t:?}");
    assert_eq!(r.e.selected_text().trim(), "quick");
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&r.e), before, "one undo step");
    let mut r = super::scenarios::Run::from_bytes(&doc, "dnd-copy");
    select_quick(&mut r);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 6 }, "to": { "para": 1, "offset": 0 } }, "mods": { "ctrl": true } }));
    let t = texts(&r.e);
    assert!(t[0].contains("quick") && t[1].contains("quick"), "{t:?}");
    let mut r = super::scenarios::Run::from_bytes(&doc, "dnd-self");
    select_quick(&mut r);
    let before = texts(&r.e);
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 5 }, "to": { "para": 0, "offset": 8 } } }));
    assert_eq!(texts(&r.e), before);
}

#[test]
fn dragging_a_whole_control_keeps_its_properties() {
    use serde_json::json;
    let mut r = super::scenarios::Run::from_bytes(&blank_with("The quick brown fox.\nJumps over."), "dnd-control");
    r.step(&json!({ "click": { "para": 0, "offset": 4 } }));
    for _ in 0..5 {
        r.step(&json!({ "key": "ArrowRight", "mods": { "shift": true } }));
    }
    let wrap = crate::api::command::InsertControl {
        kind: crate::api::value::ControlKind::RichText,
        tag: Some("q".into()),
        title: None,
        lock: None,
        placeholder: None,
        binding: None,
        options: None,
        placeholder_image: None,
        wrap_selection: Some(true),
        float: None,
    };
    r.e.execute(Command::InsertControl(wrap), fonts());
    r.step(&json!({ "click": { "para": 0, "offset": 3 } }));
    for _ in 0..8 {
        r.step(&json!({ "key": "ArrowRight", "mods": { "shift": true } }));
    }
    assert!(r.e.selected_text().contains("quick"), "{:?}", r.e.selected_text());
    r.step(&json!({ "drag": { "from": { "para": 0, "offset": 6 }, "to": { "para": 1, "offset": "end" } } }));
    assert!(texts(&r.e)[1].contains("quick"), "{:?}", texts(&r.e));
    let tags: Vec<_> = r.e.document().controls.values().map(|c| c.props.tag.clone()).collect();
    assert_eq!(tags, vec![Some("q".to_string())], "the moved control keeps its definition");
}

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
    use crate::editing::keymap::Mods;
    let mut e = typed_table(&[&["a1", "a2"], &["b1", "b2"], &["c1", "c2"]]);
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
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(table_texts(&e), [["a1", "a2"], ["c1", "c2"], ["b1", "b2"]]);
    let saved = crate::docx::write::save(e.document()).unwrap();
    assert_eq!(table_texts(&super::super::Editor::open(&saved, fonts()).unwrap()), table_texts(&e), "saved in the new order");
}

#[test]
fn double_click_auto_fits_a_column() {
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
fn fitting_columns_together_gives_the_room_one_frees_to_another() {
    let mut e = typed_table(&[
        &["A longer piece of text in column one that wraps onto a second line here.", "short", "x"],
        &["A longer piece of text in column one that wraps onto a second line here.", "short", "x"],
    ]);
    let grid = |e: &super::super::Editor| {
        e.document().sections[0].blocks.iter().find_map(|b| if let crate::docx::Block::Table(t) = b { Some(t.grid.clone()) } else { None }).unwrap()
    };
    let table = e.index().table_ids().next().expect("the table");
    let before = grid(&e);
    e.auto_fit_columns_to_contents(table, (0, 2), fonts());
    let g = grid(&e);
    assert!(g[1] < before[1] && g[2] < before[2], "the short ones narrow: {before:?} → {g:?}");
    assert!(g[0] > before[0] + 50.0, "the long one takes the room they left: {before:?} → {g:?}");
    assert!(g.iter().sum::<f64>() <= 468.0 + 12.0, "at most the text width and the padding: {g:?}");
}

#[test]
fn ctrl_alt_5_and_tab_go_through_floats() {
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

#[test]
fn after_a_page_break_the_caret_shows_on_the_next_page() {
    let mut e = crate::editing::Editor::open(&blank_with("Conversation"), fonts()).unwrap();
    e.execute(super::mv(MoveBy::Story, Dir::Forward, false), fonts());
    let p0 = e.caret_view().expect("caret").page;
    e.execute(Command::InsertPageBreak, fonts());
    assert_eq!(e.caret_view().expect("caret").page, p0 + 1, "the caret follows the break");
    assert_eq!(texts(&e), ["Conversation\u{c}", ""], "the break ends its paragraph");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), ["Conversation"], "one undo step");
    e.execute(Command::Redo(Default::default()), fonts());
    e.execute(Command::InsertText("abc".into()), fonts());
    let typed = e.caret_view().expect("caret");
    assert_eq!(typed.page, p0 + 1);
    assert!(typed.x > 40.0, "the caret is after the typed text: {typed:?}");
    e.execute(Command::InsertParagraph, fonts());
    e.execute(super::mv(MoveBy::Line, Dir::Back, false), fonts());
    assert_eq!(e.caret_view().expect("caret").page, p0 + 1);
}

#[test]
fn a_page_break_with_split_pg_break_and_para_mark_keeps_its_paragraph() {
    let mut doc = crate::docx::read_editable(&blank_with("Conversation\nB")).unwrap();
    doc.settings.policy.mode = 11;
    doc.settings.policy.split_pg_break_and_para_mark = true;
    let mut e = crate::editing::Editor::with_document(doc, fonts());
    e.execute(super::mv(MoveBy::LineEdge, Dir::Forward, false), fonts());
    e.execute(Command::InsertPageBreak, fonts());
    assert_eq!(texts(&e), ["Conversation\u{c}", "B"], "only the break");
    let mut pages = vec![e.caret_view().expect("caret").page];
    for d in [Dir::Back, Dir::Forward, Dir::Forward, Dir::Back] {
        e.execute(super::mv(MoveBy::Line, d, false), fonts());
        pages.push(e.caret_view().expect("caret").page);
    }
    assert_eq!(pages, [1, 0, 1, 1, 1], "the mark's line on the next page");
}

#[test]
fn tab_changes_the_level_of_list_items() {
    use super::super::position::Position;
    use crate::editing::keymap::Mods;
    let mut e = open("FX-LISTS.docx");
    let level = |e: &super::super::Editor, i: usize| match &e.document().sections[0].blocks[i] {
        crate::docx::Block::Para(p) => p.style.num.map(|(_, l)| l),
        _ => None,
    };
    let at = |e: &super::super::Editor, i: usize| Position::text(StoryId::Body, super::super::tests::para_id(e, i), Unit(0));
    assert_eq!((level(&e, 2), level(&e, 3)), (Some(0), Some(0)), "1. and 2.");
    e.set_caret_for_benchmark(Selection::Caret(at(&e, 2)));
    let left = |e: &super::super::Editor, i: usize| match &e.document().sections[0].blocks[i] {
        crate::docx::Block::Para(p) => p.style.ind_left,
        _ => 0.0,
    };
    let was = left(&e, 3);
    let tab = e.key_command("Tab", Mods::default()).unwrap();
    assert!(matches!(e.execute(tab, fonts()), super::super::Outcome::Changed(_)), "the first item");
    assert_eq!((level(&e, 2), left(&e, 3) > was), (Some(0), true), "the first item takes the whole list along");
    e.execute(Command::Undo(Default::default()), fonts());
    e.indent_keys = false;
    let tab = e.key_command("Tab", Mods::default()).unwrap();
    e.execute(tab, fonts());
    assert_eq!(level(&e, 2), Some(1), "with the AutoFormat option off, a level down");
    e.execute(Command::Undo(Default::default()), fonts());
    e.indent_keys = true;
    let before = texts(&e);
    e.set_caret_for_benchmark(Selection::Text {
        anchor: at(&e, 3),
        focus: Position::text(StoryId::Body, super::super::tests::para_id(&e, 4), Unit(2)),
        unit: Default::default(),
    });
    let tab = e.key_command("Tab", Mods::default()).unwrap();
    e.execute(tab, fonts());
    assert_eq!((level(&e, 3), level(&e, 4)), (Some(1), Some(2)), "both items");
    assert_eq!(texts(&e), before, "the text stays");
    let back = e.key_command("Tab", Mods { shift: true, ..Mods::default() }).unwrap();
    e.execute(back, fonts());
    assert_eq!((level(&e, 3), level(&e, 4)), (Some(0), Some(1)));
}

#[test]
fn tab_in_a_cells_list_item_changes_its_level() {
    use crate::editing::keymap::Mods;
    let mut e = open("FX-LISTS.docx");
    let found = (0..e.document().sections.len()).find_map(|si| {
        e.document().sections[si].blocks.iter().find_map(|b| match b {
            crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()).find_map(|b| match b {
                crate::docx::Block::Para(p) if p.style.num.is_some_and(|(_, l)| l == 1) => Some(crate::layout::ParaId(p.id)),
                _ => None,
            }),
            _ => None,
        })
    });
    let para = found.expect("a level-two item in a cell");
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(0))));
    let tab = e.key_command("Tab", Mods::default()).unwrap();
    assert!(matches!(tab, Command::IncreaseIndent), "the list's Tab, not the next cell: {tab:?}");
    e.execute(tab, fonts());
    assert_eq!(e.index().para(e.document(), para).and_then(|p| p.style.num).map(|(_, l)| l), Some(2));
}

#[test]
fn select_all_delete_across_sections() {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-feature-showcase.docx")).unwrap();
    let mut e = super::super::Editor::open(&bytes, fonts()).unwrap();
    let before = texts(&e);
    let pages = |e: &super::super::Editor| e.document().sections.iter().map(|s| s.page()).collect::<Vec<_>>();
    let was = pages(&e);
    assert!(was.len() > 1);
    e.execute(Command::SelectAll, fonts());
    e.execute(crate::editing::tests::del(Dir::Forward, false), fonts());
    assert_eq!(texts(&e), vec![String::new()]);
    assert_eq!(pages(&e), vec![was.last().unwrap().clone()]);
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let again = super::super::Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(texts(&again), vec![String::new()]);
    assert_eq!(pages(&again), pages(&e));
    let xml = crate::docx::write::main_xml(e.document()).unwrap().unwrap();
    assert_eq!(xml.matches("<w:sectPr").count(), 1, "{xml}");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), before);
    assert_eq!(pages(&e), was);
    assert!(crate::docx::write::main_xml(e.document()).unwrap().is_none(), "undone, nothing to save");
}

#[test]
fn backspace_and_delete_take_a_section_break() {
    use crate::docx::Block;
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-feature-showcase.docx")).unwrap();
    let mut e = super::super::Editor::open(&bytes, fonts()).unwrap();
    let pages = |e: &super::super::Editor| e.document().sections.iter().map(|s| s.page()).collect::<Vec<_>>();
    let para = |e: &super::super::Editor, k: usize, last: bool| {
        let bl = &e.document().sections[k].blocks;
        match if last { bl.last() } else { bl.first() } {
            Some(Block::Para(p)) => (crate::layout::ParaId(p.id), super::super::step::para_len(p)),
            _ => panic!("a paragraph"),
        }
    };
    let (was, before) = (pages(&e), texts(&e));
    let n = was.len();
    let sects = |e: &super::super::Editor| crate::docx::write::main_xml(e.document()).unwrap().unwrap().matches("<w:sectPr").count();
    for (back, k) in [(true, 1), (false, 1)] {
        let (p, len) = para(&e, k, !back);
        e.selection = Selection::Caret(super::super::position::Position::text(StoryId::Body, p, if back { Unit(0) } else { len }));
        let paras = texts(&e).len();
        e.execute(crate::editing::tests::del(if back { Dir::Back } else { Dir::Forward }, false), fonts());
        assert_eq!(texts(&e).len() + 1, paras, "joined");
        assert_eq!(sects(&e), pages(&e).len());
        let again = super::super::Editor::open(&crate::docx::write::save(e.document()).unwrap(), fonts()).unwrap();
        assert_eq!((pages(&again), texts(&again)), (pages(&e), texts(&e)), "reopened");
    }
    assert_eq!(pages(&e), vec![was[1].clone(), was[3].clone()]);
    let (p, len) = para(&e, n - 3, true);
    e.selection = Selection::Caret(super::super::position::Position::text(StoryId::Body, p, len));
    e.execute(crate::editing::tests::del(Dir::Forward, false), fonts());
    assert_eq!(pages(&e).len(), n - 2);
    e.execute(Command::Undo(Default::default()), fonts());
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((pages(&e), texts(&e)), (was, before));
}

#[test]
fn hyphenation_settings_change_undo_and_save_in_schema_order() {
    use crate::api::state::{CommandValue, Tri};
    use crate::api::value::Points;
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-TEXT.docx")).expect("fixture");
    let mut e = crate::editing::Editor::open(&bytes, fonts()).expect("opens");
    e.execute(Command::SetAutoHyphenation(true), fonts());
    e.execute(Command::SetConsecutiveHyphenLimit(2), fonts());
    e.execute(Command::SetHyphenationZone(Points(9.0)), fonts());
    e.execute(Command::SetHyphenateCaps(false), fonts());
    let s = &e.document().settings;
    assert_eq!((s.auto_hyphenation, s.consecutive_hyphen_limit, s.hyphenation_zone, s.do_not_hyphenate_caps), (true, 2, 9.0, true));
    let f = e.format_state();
    let states = e.command_states(f.as_ref(), super::super::state::StateFlags { read_only: false });
    let id = |c: Command| c.id();
    assert_eq!(states[&id(Command::SetAutoHyphenation(true))].checked, Some(true));
    assert_eq!(states[&id(Command::SetHyphenateCaps(true))].checked, Some(false));
    assert_eq!(states[&id(Command::SetHyphenationZone(Points(0.0)))].value, Some(CommandValue::Length(Tri::Uniform(Points(9.0)))));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&saved[..])).expect("zip");
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut z.by_name("word/settings.xml").expect("settings"), &mut xml).expect("reads");
    let at = |el: &str| xml.find(el).unwrap_or_else(|| panic!("{el} in {xml}"));
    assert!(at("<w:defaultTabStop") < at("<w:autoHyphenation/>"));
    assert!(at("<w:autoHyphenation/>") < at("<w:consecutiveHyphenLimit w:val=\"2\"/>"));
    assert!(at("<w:consecutiveHyphenLimit") < at("<w:hyphenationZone w:val=\"180\"/>"));
    assert!(at("<w:hyphenationZone") < at("<w:doNotHyphenateCaps/>"));
    let again = crate::docx::read_editable(&saved).expect("reads");
    assert_eq!((again.settings.auto_hyphenation, again.settings.consecutive_hyphen_limit), (true, 2));
    for _ in 0..4 {
        e.execute(Command::Undo(Default::default()), fonts());
    }
    let s = &e.document().settings;
    assert_eq!((s.auto_hyphenation, s.consecutive_hyphen_limit, s.do_not_hyphenate_caps), (false, 0, false));
    assert!(matches!(e.execute(Command::SetHyphenationZone(Points(-1.0)), fonts()), super::super::commands::Outcome::Refused(_)));
}

#[test]
fn a_list_reapplied_after_backspacing_out_indents_again() {
    let mut e = open("FX-TEXT.docx");
    let para = super::super::tests::para_id(&e, 1);
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, para, Unit(0))));
    let ind = |e: &super::super::Editor| e.index.para(&e.doc, para).map(|p| (p.style.ind_left, p.style.ind_first));
    e.execute(Command::SetList(Some(crate::api::command::ListKind::Numbered)), fonts());
    let listed = ind(&e);
    e.execute(crate::editing::tests::del(Dir::Back, false), fonts());
    e.execute(crate::editing::tests::del(Dir::Back, false), fonts());
    assert_ne!(ind(&e), listed, "backspace took the list off");
    e.execute(Command::SetList(Some(crate::api::command::ListKind::Numbered)), fonts());
    assert_eq!(ind(&e), listed);
}

#[test]
fn reformatting_a_paragraph_in_a_division_keeps_the_divisions_margins() {
    use std::io::{Read, Write};
    let body = r#"<w:p><w:r><w:t>Before.</w:t></w:r></w:p><w:p><w:pPr><w:divId w:val="7"/></w:pPr><w:r><w:t>In the division.</w:t></w:r></w:p>"#;
    let web = r#"<w:webSettings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:divs><w:div w:id="7"><w:marLeft w:val="1440"/><w:marRight w:val="720"/><w:marTop w:val="240"/><w:marBottom w:val="0"/></w:div></w:divs></w:webSettings>"#;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(super::blank_body(body))).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "word/_rels/document.xml.rels" {
            let rel = r#"<Relationship Id="rIdWeb" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/webSettings" Target="webSettings.xml"/></Relationships>"#;
            data = String::from_utf8(data).unwrap().replace("</Relationships>", rel).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.start_file("word/webSettings.xml", zip::write::SimpleFileOptions::default()).unwrap();
    out.write_all(web.as_bytes()).unwrap();
    let mut e = super::super::Editor::open(&out.finish().unwrap().into_inner(), fonts()).unwrap();
    let margins = |e: &super::super::Editor| {
        let crate::docx::Block::Para(p) = &e.document().sections[0].blocks[1] else { panic!("a paragraph") };
        let s = p.para_style_in(e.document().review.view);
        (s.ind_left, s.ind_right, s.space_before, s.jc)
    };
    let read = margins(&e);
    assert_eq!((read.0, read.1, read.2), (72.0, 36.0, 12.0));
    e.execute(crate::editing::tests::mv(MoveBy::Story, Dir::Forward, false), fonts());
    e.execute(Command::SetAlignment(crate::api::value::Alignment::Center), fonts());
    assert_eq!(margins(&e), (72.0, 36.0, 12.0, crate::docx::props::Jc::Center));
}

#[test]
fn a_float_dropped_on_the_next_page_between_idles_lands_there() {
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    use serde_json::json;
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let text: Vec<String> =
        (0..40).map(|i| format!("Paragraph {i} lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod.")).collect();
    let mut r = super::scenarios::Run::from_bytes(&super::blank_with(&text.join("\n")), "drop-next-page");
    r.step(&json!({ "click": { "para": 2, "offset": 3 } }));
    r.e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data), alt_text: None }), fonts());
    r.e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::Square), fonts());
    while r.e.idle(fonts()) {}
    let g = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
    let v: Vec<f64> = g["geometry"]["floats"][0]["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let (x, y) = ((v[0] + v[2]) / 2.0, (v[1] + v[3]) / 2.0);
    // The host's moves come faster than idle layout: the pages after each preview are still provisional.
    r.e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    for k in 1..=40 {
        r.e.pointer_input(PointerKind::Move, x, y + 20.0 * k as f64, 0, Mods::default(), fonts());
    }
    r.e.pointer_input(PointerKind::Up, x, y + 800.0, 0, Mods::default(), fonts());
    while r.e.idle(fonts()) {}
    let g = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
    let f = &g["geometry"]["floats"][0];
    assert_eq!(f["page"], json!(1), "{f}");
    let top = g["geometry"]["pages"][1][1].as_f64().unwrap();
    assert!(f["rect"][1].as_f64().unwrap() > top, "{f}");
    r.e.execute(Command::Undo(Default::default()), fonts());
    while r.e.idle(fonts()) {}
    let g = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
    assert_eq!(g["geometry"]["floats"][0]["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>(), v, "undone");
}

#[test]
fn an_inserted_text_box_draws_as_it_reopens() {
    use super::super::position::{Position, StoryId};
    let mut e = super::super::Editor::open(&super::blank_with("Body text here"), fonts()).expect("opens");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 0), Unit(2))));
    e.execute(Command::InsertTextBox, fonts());
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = super::super::Editor::open(&bytes, fonts()).expect("reopens");
    let look = |e: &super::super::Editor| {
        let f = body_floats(e).remove(0);
        let sp = f.shape.as_ref().and_then(|s| s.sp()).cloned().expect("a shape");
        (format!("{:?} {:?} {:?}", sp.fill, sp.line, sp.body), f.text_box.as_ref().map(|t| (t.insets, t.outline)))
    };
    assert_eq!(look(&e), look(&again));
    assert!(look(&e).0.starts_with("Some(Solid("), "{:?}", look(&e));
}

#[test]
fn an_inserted_pictures_alt_text_and_aspect_lock_save_inline_and_floating() {
    use super::super::position::{Position, StoryId};
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut e = super::super::Editor::open(&super::blank_with("Body text here"), fonts()).expect("opens");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 0), Unit(2))));
    let alt = Some("A black \"square\"".to_string());
    e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data), alt_text: alt }), fonts());
    let picture = |e: &super::super::Editor| {
        e.document().sections[0].blocks.iter().find_map(|b| match b {
            crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                crate::docx::Inline::Picture(p, _) => Some((p.alt.to_string(), p.lock_aspect)),
                crate::docx::Inline::Float(f, _) => Some((f.alt.to_string(), f.lock_aspect)),
                _ => None,
            }),
            _ => None,
        })
    };
    let want = Some(("A black \"square\"".to_string(), true));
    assert_eq!(picture(&e), want, "inserted");
    let reopened = |e: &super::super::Editor| {
        let bytes = crate::docx::write::save(e.document()).expect("saves");
        super::super::Editor::open(&bytes, fonts()).expect("reopens")
    };
    assert_eq!(picture(&reopened(&e)), want, "inline, reopened");
    let out = e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::Square), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert_eq!(picture(&e), want, "floated");
    assert_eq!(picture(&reopened(&e)), want, "floating, reopened");
}

#[test]
fn a_picture_floated_from_its_paragraphs_first_line_stays_below_the_paragraph_before() {
    use super::super::position::{Position, StoryId};
    use crate::api::value::Points;
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let long = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.";
    let xml: String = [long; 4].iter().map(|t| format!("<w:p><w:pPr><w:spacing w:after=\"0\"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>")).collect();
    let mut e = super::super::Editor::open(&super::blank_body(&xml), fonts()).expect("opens");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 1), Unit(0))));
    e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data), alt_text: None }), fonts());
    let size =
        crate::api::command::ObjectSize { width: Some(Points(150.0)), height: Some(Points(90.0)), lock_aspect: Some(false), ..Default::default() };
    e.execute(Command::SetObjectSize(size), fonts());
    let first = |e: &mut super::super::Editor| {
        while e.idle(fonts()) {}
        let g = crate::editing::inspect::inspect(e, "geometry", true);
        g["geometry"]["lines"].as_array().unwrap().iter().filter(|l| l["para"] == serde_json::json!(0)).cloned().collect::<Vec<_>>()
    };
    let before = first(&mut e);
    e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::Square), fonts());
    let f = body_floats(&e).remove(0);
    assert!(f.v_offset > 0.0, "below its paragraph's top: {}", f.v_offset);
    assert_eq!(first(&mut e), before, "the paragraph before keeps its lines");
}

#[test]
fn a_delete_across_a_locked_paragraph_is_refused() {
    use super::super::position::Position;
    let body = concat!(
        r#"<w:p><w:r><w:t>First paragraph</w:t></w:r></w:p>"#,
        r#"<w:sdt><w:sdtPr><w:lock w:val="sdtLocked"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Locked paragraph</w:t></w:r></w:p></w:sdtContent></w:sdt>"#,
        r#"<w:p><w:r><w:t>Last paragraph</w:t></w:r></w:p>"#,
    );
    let mut e = super::super::Editor::open(&super::blank_body(body), fonts()).expect("opens");
    let before = texts(&e);
    let (a, b) = (super::para_id(&e, 0), super::para_id(&e, 2));
    e.set_caret_for_benchmark(Selection::range(
        Position::text(StoryId::Body, a, Unit(2)),
        Position::text(StoryId::Body, b, Unit(2)),
        crate::editing::selection::Granularity::Char,
    ));
    e.execute(crate::editing::tests::del(Dir::Back, false), fonts());
    assert_eq!(texts(&e), before, "the locked paragraph in the middle stays");
}

#[test]
fn a_list_made_in_a_header_is_numbered() {
    use super::super::position::Position;
    let mut e = open("FX-STORIES.docx");
    let header = e.document().stories.keys().filter(|n| n.contains("header")).min().cloned().expect("a header");
    let story = e.index().header_footer(&header).expect("its story");
    let para =
        e.index().para_ids().find(|id| e.index().para_slot(*id).is_some_and(|s| e.index().story(s.container) == story)).expect("a header paragraph");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(story, para, Unit(0))));
    let out = e.execute(Command::SetList(Some(crate::api::command::ListKind::Numbered)), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let label = e.index().para(e.document(), para).and_then(|p| p.list.as_ref()).map(|l| l.text.clone());
    assert_eq!(label.as_deref(), Some("1."), "the header's list has its label");
}

#[test]
fn a_paragraph_cut_and_pasted_in_a_header_or_note_is_a_tracked_move() {
    use super::super::index::Container;
    use super::super::position::Position;
    use crate::docx::review::MarkKind;
    let mut r = super::scenarios::Run::open("review/FX-REVIEW-MOVES-STORIES.docx", "story-moves");
    r.e.execute(Command::SetTrackChanges(true), fonts());
    let header = r.e.document().stories.keys().find(|n| n.contains("header")).cloned().expect("a header");
    let stories = [StoryId::Body, r.e.index().header_footer(&header).expect("its story"), StoryId::Footnote(1)];
    for story in stories {
        let container = if story == StoryId::Body { Container::Section(0) } else { Container::Story(story) };
        let paras: Vec<_> =
            r.e.index()
                .blocks(r.e.document(), container)
                .expect("its blocks")
                .iter()
                .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(crate::layout::ParaId(p.id)) } else { None })
                .collect();
        r.e.set_caret_for_benchmark(Selection::range(
            Position::text(story, paras[0], Unit(0)),
            Position::text(story, paras[1], Unit(0)),
            crate::editing::selection::Granularity::Char,
        ));
        r.step(&serde_json::json!({ "clipboard": "cut" }));
        r.e.set_caret_for_benchmark(Selection::Caret(Position::text(story, paras[2], Unit(0))));
        r.step(&serde_json::json!({ "clipboard": "paste" }));
        let blocks = r.e.index().blocks(r.e.document(), container).expect("its blocks");
        let kinds: Vec<MarkKind> = blocks
            .iter()
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
            .flat_map(|p| p.inlines.iter().filter_map(|i| i.run()).flat_map(|r| r.rev.ins.iter().chain(r.rev.del.iter()).map(|m| m.kind)))
            .collect();
        assert!(kinds.contains(&MarkKind::MoveFrom) && kinds.contains(&MarkKind::MoveTo), "{story:?}: {kinds:?}");
        assert!(!kinds.contains(&MarkKind::Insert) && !kinds.contains(&MarkKind::Delete), "{story:?}: a move, as Word records it: {kinds:?}");
    }
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    if let Some(d) = std::env::var_os("RT_KEEP") {
        std::fs::write(std::path::Path::new(&d).join("FX-REVIEW-MOVES-STORIES.moved.docx"), &bytes).expect("writes");
    }
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a package");
    for part in ["word/document.xml", header.trim_start_matches('/'), "word/footnotes.xml"] {
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut z.by_name(part).expect("the part"), &mut xml).expect("text");
        assert!(xml.contains("<w:moveFrom ") && xml.contains("<w:moveTo ") && xml.contains("<w:moveFromRangeStart "), "{part}");
        // Text moved away is `w:t`, as Word writes it: Word reports a `w:delText` in a `w:moveFrom` as unreadable.
        let moved_away = xml.split("<w:moveFrom ").skip(1).filter_map(|s| s.split("</w:moveFrom>").next());
        assert!(moved_away.clone().all(|s| !s.contains("delText")), "{part}");
    }
}

#[test]
fn pasting_cut_notes_after_a_new_one_keeps_every_note() {
    use super::para_id;
    let mut e = super::super::Editor::open(&super::blank_body("<w:p><w:r><w:t>one two three four</w:t></w:r></w:p>"), fonts()).expect("opens");
    let refs = |e: &super::super::Editor| -> Vec<i64> {
        e.document().sections[0]
            .blocks
            .iter()
            .flat_map(|b| match b {
                crate::docx::Block::Para(p) => {
                    p.inlines.iter().filter_map(|i| if let crate::docx::Inline::NoteRef { id, .. } = i { Some(*id) } else { None }).collect()
                }
                _ => Vec::new(),
            })
            .collect()
    };
    let p = para_id(&e, 0);
    for at in [3, 8] {
        e.select_range(p, Unit(at), Unit(at), fonts());
        e.execute(Command::InsertFootnote, fonts());
    }
    e.select_range(p, Unit(0), Unit(9), fonts());
    e.execute(Command::Cut, fonts());
    let p = para_id(&e, 0);
    e.select_range(p, Unit(11), Unit(11), fonts());
    e.execute(Command::InsertFootnote, fonts());
    let p = para_id(&e, 0);
    e.select_range(p, Unit(0), Unit(0), fonts());
    e.execute(Command::Paste(Default::default()), fonts());
    let mut ids = refs(&e);
    let mut notes: Vec<i64> = e.document().footnotes.keys().copied().filter(|k| *k > 0).collect();
    ids.sort();
    notes.sort();
    assert_eq!(ids, notes, "each note has its own reference");
    assert_eq!(ids.len(), 3);
}

#[test]
fn insert_and_save_take_a_wmf() {
    // A placeable WMF of 100 units at 72 to the inch holding nothing but its end.
    let mut data = 0x9AC6_CDD7u32.to_le_bytes().to_vec();
    for v in [0i16, 0, 0, 100, 100] {
        data.extend(v.to_le_bytes());
    }
    data.extend(72u16.to_le_bytes());
    data.extend([0u8; 6]);
    data.extend([1u16, 9, 0x300].iter().flat_map(|v| v.to_le_bytes()));
    data.extend(12u32.to_le_bytes());
    data.extend([0u8; 6]);
    data.extend([3u32.to_le_bytes().as_slice(), &[0, 0]].concat());
    let mut e = open("FX-STORIES.docx");
    let out =
        e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data.clone()), alt_text: None }), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let types = crate::docx::package::read(&saved, &[], "[Content_Types].xml").expect("reads").expect("types");
    assert!(types.contains("Extension=\"wmf\"") && types.contains("image/x-wmf"), "{types}");
    assert!(crate::docx::read(&saved).expect("reopens").images.iter().any(|i| i == &data));
}

#[test]
fn dropped_text_is_selected_from_the_insertion_point() {
    use super::super::input::PointerKind;
    let body = "<w:p><w:r><w:t>word </w:t></w:r></w:p><w:p><w:r><w:t>e\u{301}x</w:t></w:r></w:p>";
    let mut e = super::super::Editor::open(&super::blank_body(body), fonts()).expect("opens");
    e.complete(fonts());
    let at = |e: &mut super::super::Editor, para: usize, u: u32, dx: f64| {
        let id = super::para_id(e, para);
        let g = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
        let l = g.lines.iter().find(|l| l.shows(id)).expect("a line");
        let x = l.stops.iter().find(|s| s.para == id && s.unit == Unit(u)).expect("a stop").x;
        e.stack().to_doc(0, x + dx, (l.top + l.bottom) / 2.0).expect("on the stack")
    };
    let first = super::para_id(&e, 0);
    e.select_range(first, Unit(0), Unit(4), fonts());
    // Onto the accent: between the base letter and its combining mark.
    let (from, to) = (at(&mut e, 0, 2, 1.0), at(&mut e, 1, 1, 0.0));
    let m = super::super::keymap::Mods::default();
    e.pointer_input(PointerKind::Down, from.0, from.1, 1, m, fonts());
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, m, fonts());
    e.pointer_input(PointerKind::Up, to.0, to.1, 0, m, fonts());
    assert_eq!(texts(&e), ["", "word e\u{301}x"]);
    let (a, b) = e.selection().ordered(e.index()).expect("a range");
    assert_eq!((a.para_offset().map(|p| p.1), b.para_offset().map(|p| p.1)), (Some(Unit(0)), Some(Unit(4))), "{:?}", e.selection());
}

#[test]
fn change_case_leaves_field_code_alone() {
    use crate::api::command::{CaseChange, Hyperlink};
    use crate::docx::fields;
    let mut e = super::super::Editor::open(&blank_with("hello "), fonts()).expect("opens");
    e.execute(
        Command::InsertHyperlink(Hyperlink { address: "https://example.com/Path".into(), text: Some("click".into()), ..Default::default() }),
        fonts(),
    );
    e.execute(Command::SelectAll, fonts());
    e.execute(Command::ChangeCase(CaseChange::Upper), fonts());
    let id = super::texts_ids(&e)[0];
    let p = e.index().para(e.document(), id).expect("the paragraph");
    let s = fields::spans(&p.inlines)[0];
    let code = fields::code_text(&p.inlines, s);
    assert!(code.contains("https://example.com/Path"), "the code is recased: {code}");
    assert_eq!(fields::result_text(&p.inlines, s), "CLICK");
    assert!(texts(&e)[0].ends_with("HELLO "), "{:?}", texts(&e));
}

#[test]
fn repeat_reapplies_formatting() {
    use crate::api::value::{AutoOr, Color};
    use crate::docx::Inline;
    let mut e = super::super::Editor::open(&blank_with("ab\ncd"), fonts()).expect("opens");
    let select = |e: &mut super::super::Editor, n| {
        let p = super::para_id(e, n);
        e.set_caret_for_benchmark(Selection::Text {
            anchor: super::super::position::Position::text(StoryId::Body, p, Unit(0)),
            focus: super::super::position::Position::text(StoryId::Body, p, Unit(2)),
            unit: super::super::selection::Granularity::Char,
        });
    };
    let color = |e: &super::super::Editor, n| {
        let p = e.index().para(e.document(), super::para_id(e, n)).expect("the paragraph");
        p.inlines.iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.color) } else { None }).expect("a run")
    };
    select(&mut e, 0);
    assert!(matches!(
        e.execute(Command::SetFontColor(AutoOr::Value(crate::api::drawing::ColourValue::rgb(Color(0xC00000)))), fonts()),
        super::super::commands::Outcome::Changed(_)
    ));
    select(&mut e, 1);
    let o = e.execute(Command::Repeat, fonts());
    assert_eq!(texts(&e), ["ab", "cd"], "nothing typed again: {o:?}");
    assert_eq!(color(&e, 1), Some(0xC00000), "{o:?}");
}

#[test]
fn a_heavy_tab_leader_is_kept() {
    use crate::api::command::AddTabStop;
    use crate::api::value::{Points, TabLeader};
    let xml = r#"<w:p><w:pPr><w:tabs><w:tab w:val="right" w:leader="heavy" w:pos="9360"/></w:tabs></w:pPr><w:r><w:t>text</w:t></w:r></w:p>"#;
    let mut e = super::super::Editor::open(&super::blank_body(xml), fonts()).expect("opens");
    let p = super::para_id(&e, 0);
    e.set_caret_for_benchmark(Selection::Caret(super::super::position::Position::text(StoryId::Body, p, Unit(0))));
    let saved = |e: &super::super::Editor| {
        let bytes = crate::docx::write::save(e.document()).expect("saves");
        crate::docx::package::read(&bytes, &[], "word/document.xml").expect("reads").expect("the part")
    };
    e.execute(Command::AddTabStop(AddTabStop { position: Points(100.0), alignment: None, leader: None }), fonts());
    assert!(saved(&e).contains("w:leader=\"heavy\""), "{}", saved(&e));
    e.execute(Command::AddTabStop(AddTabStop { position: Points(200.0), alignment: None, leader: Some(TabLeader::Heavy) }), fonts());
    assert_eq!(saved(&e).matches("w:leader=\"heavy\"").count(), 2, "{}", saved(&e));
    let p = e.index().para(e.document(), p).expect("the paragraph");
    assert!(p.style.tabs.iter().any(|t| t.pos == 200.0 && t.leader == crate::docx::props::Leader::Heavy), "{:?}", p.style.tabs);
}

#[test]
fn typing_before_a_combining_mark_puts_the_caret_after_the_cluster() {
    use super::super::position::Position;
    let body = "<w:p><w:r><w:t>e\u{301}x</w:t></w:r></w:p>";
    let mut e = super::super::Editor::open(&crate::layout::tests::shapes::bytes(body).expect("a package"), fonts()).expect("opens");
    let para = super::para_id(&e, 0);
    let at = |e: &mut super::super::Editor, u: u32| e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para, Unit(u))));
    let caret = |e: &super::super::Editor| e.selection().focus().and_then(|p| p.para_offset()).map(|p| p.1.0);
    at(&mut e, 1);
    e.execute(Command::InsertText("a".into()), fonts());
    assert_eq!(caret(&e), Some(3), "a joins the accent's cluster");
    at(&mut e, 0);
    e.execute(Command::InsertText("b".into()), fonts());
    assert_eq!(caret(&e), Some(1), "before a plain letter");
    at(&mut e, 5);
    e.execute(Command::InsertText("c".into()), fonts());
    assert_eq!(caret(&e), Some(6), "at the end");
}

#[test]
fn dragging_the_selection_out_carries_it_and_a_move_deletes_it() {
    use super::super::position::Position;
    use crate::layout::{ParaId, Unit};
    use serde_json::json;
    let doc = blank_with("The quick brown fox.\nJumps over.");
    let mut r = super::scenarios::Run::from_bytes(&doc, "drag-out");
    r.step(&json!({ "click": { "para": 0, "offset": 4 } }));
    for _ in 0..5 {
        r.step(&json!({ "key": "ArrowRight", "mods": { "shift": true } }));
    }
    let out = r.e.drag_out().expect("a selection");
    assert_eq!((out.text.as_str(), out.can_move), ("quick", true));
    assert!(matches!(r.e.drag_out_end(false, fonts()), super::super::commands::Outcome::Unchanged(_)), "a copy leaves it");
    assert_eq!(texts(&r.e)[0], "The quick brown fox.");
    r.e.drag_out().expect("a selection");
    r.e.drag_out_end(true, fonts());
    assert_eq!(texts(&r.e)[0], "The brown fox.");
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&r.e)[0], "The quick brown fox.");
    r.e.drag_out().expect("a selection");
    let para = r.e.document().sections[0].blocks.iter().filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.id) } else { None }).nth(1);
    let c = r.e.caret_at(Position::text(StoryId::Body, ParaId(para.expect("two paragraphs")), Unit(0))).expect("laid out");
    let (x, y) = r.e.stack().to_doc(c.page, c.x, (c.top + c.bottom) / 2.0).expect("on the stack");
    r.e.drag_out_drop(x, y, false, fonts());
    let t = texts(&r.e);
    assert!(!t[0].contains("quick") && t[1].starts_with("quick"), "{t:?}");
    assert!(r.e.drag_out().is_some_and(|o| o.text.trim() == "quick"), "the moved text stays selected");
    let p = crate::api::command::DocumentProtection {
        kind: Some(crate::api::command::ProtectionKind::ReadOnly),
        enforce: Some(true),
        ..Default::default()
    };
    r.e.execute(Command::SetDocumentProtection(p), fonts());
    assert!(r.e.drag_out().is_some_and(|o| !o.can_move));
}

/// Document coordinates of the caret at unit `u` of the `n`th paragraph.
fn caret_point(e: &mut super::super::Editor, n: usize, u: u32) -> (f64, f64) {
    use super::super::position::Position;
    let c = e.caret_at(Position::text(StoryId::Body, super::para_id(e, n), crate::layout::Unit(u))).expect("laid out");
    e.stack().to_doc(c.page, c.x, (c.top + c.bottom) / 2.0).expect("on the stack")
}

#[test]
fn selected_cells_dragged_out_and_back_are_copied_not_moved() {
    use super::super::selection::{CellRef, CellShape};
    let cell = |t: &str| format!("<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>");
    let body = format!(
        "<w:tbl><w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl><w:p><w:r><w:t>after</w:t></w:r></w:p>",
        cell("a"),
        cell("b")
    );
    let mut e = super::super::Editor::open(&super::blank_body(&body), fonts()).expect("opens");
    e.complete(fonts());
    let table = e.index().table_ids().next().expect("a table");
    let cells = Selection::Cells { table, anchor: CellRef { row: 0, col: 0 }, focus: CellRef { row: 0, col: 1 }, shape: CellShape::Rect };
    e.set_selection(cells);
    assert!(e.drag_out().is_some_and(|o| !o.can_move), "cells are copied");
    let (x, y) = caret_point(&mut e, 2, 5);
    let out = e.drag_out_drop(x, y, false, fonts());
    assert!(matches!(out, super::super::commands::Outcome::Unchanged(_)), "no move: {out:?}");
    assert_eq!(texts(&e), ["a", "b", "after"]);
    e.set_selection(cells);
    e.drag_out().expect("the cells");
    let (x, y) = caret_point(&mut e, 0, 0);
    assert!(matches!(e.drag_out_drop(x, y, true, fonts()), super::super::commands::Outcome::Unchanged(_)), "dropped on the cells");
    e.drag_out().expect("the cells");
    let (x, y) = caret_point(&mut e, 2, 5);
    let out = e.drag_out_drop(x, y, true, fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    let t = texts(&e);
    assert!(["a", "b"].iter().all(|c| t.iter().filter(|s| s == c).count() == 2), "both cells copied: {t:?}");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(texts(&e), ["a", "b", "after"], "one undo step");
}

#[test]
fn a_block_dragged_out_and_back_is_copied() {
    use crate::editing::commands::{Dir, MoveBy};
    let mut e = super::super::Editor::open(&super::blank_in("abcdefgh\nijklmnop\nqrstuvwx\nyz", "Courier New"), fonts()).unwrap();
    e.complete(fonts());
    let mv = |e: &mut super::super::Editor, by, dir| {
        e.execute(crate::editing::tests::mv(by, dir, false), fonts());
    };
    mv(&mut e, MoveBy::Story, Dir::Back);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    e.execute(Command::SetSelectionMode(crate::api::command::SelectionMode::Block), fonts());
    mv(&mut e, MoveBy::Line, Dir::Forward);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    mv(&mut e, MoveBy::Char, Dir::Forward);
    let block = e.selection();
    assert!(matches!(block, Selection::Block { .. }), "{block:?}");
    assert!(e.drag_out().is_some_and(|o| !o.can_move && o.text == "cd\nkl"));
    let (x, y) = caret_point(&mut e, 3, 2);
    assert!(matches!(e.drag_out_drop(x, y, false, fonts()), super::super::commands::Outcome::Unchanged(_)), "no move");
    e.set_selection(block);
    e.drag_out().expect("the block");
    let (x, y) = caret_point(&mut e, 3, 2);
    let out = e.drag_out_drop(x, y, true, fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    let t = texts(&e);
    assert!(t[..3] == ["abcdefgh", "ijklmnop", "qrstuvwx"], "{t:?}");
    assert!(["cd", "kl"].iter().all(|c| t.concat().matches(c).count() == 2), "both lines copied: {t:?}");
}

#[test]
fn with_text_drag_and_drop_off_a_press_in_the_selection_places_the_caret() {
    use super::super::input::{Cursor, PointerKind};
    use super::super::keymap::Mods;
    use super::super::position::Position;
    use crate::layout::{ParaId, Unit};
    use serde_json::json;
    let doc = blank_with("The quick brown fox.\nJumps over.");
    let mut r = super::scenarios::Run::from_bytes(&doc, "no-drag");
    r.step(&json!({ "click": { "para": 0, "offset": 4 } }));
    for _ in 0..5 {
        r.step(&json!({ "key": "ArrowRight", "mods": { "shift": true } }));
    }
    let para = r.e.document().sections[0].blocks.iter().find_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.id) } else { None });
    let c = r.e.caret_at(Position::text(StoryId::Body, ParaId(para.expect("a paragraph")), Unit(6))).expect("laid out");
    let (x, y) = r.e.stack().to_doc(c.page, c.x + 0.5, (c.top + c.bottom) / 2.0).expect("on the stack");
    assert_eq!(r.e.hover(x, y, Mods::default(), fonts()), Cursor::Default, "the arrow over the selection");
    r.e.set_text_drag_and_drop(false);
    assert!(!r.e.text_drag_and_drop());
    assert_eq!(r.e.hover(x, y, Mods::default(), fonts()), Cursor::Text, "the I-beam");
    assert!(!r.e.right_press(x, y, fonts()));
    assert!(r.e.drag_out().is_none());
    r.e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    r.e.pointer_input(PointerKind::Up, x, y, 1, Mods::default(), fonts());
    assert!(r.e.selection().is_collapsed(), "the caret: {:?}", r.e.selection());
}
