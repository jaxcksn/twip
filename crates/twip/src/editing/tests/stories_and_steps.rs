//! Steps, the index, integrity and inspection over every story: the sub-template flow, text boxes, headers

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::super::index::{Container, DocIndex};
use super::super::inspect::inspect;
use super::super::integrity;
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::super::step::{Step, para_len};
use super::{fonts, open};
use crate::docx::{Block, Document, Float, Inline, Paragraph, Run};
use crate::layout::incremental::Change;
use crate::layout::{FloatId, ParaId, Unit};

fn read(name: &str) -> Document {
    open(name).document().clone()
}

fn blank_para_like(p: &Paragraph) -> Paragraph {
    let mut q = Paragraph { inlines: Vec::new(), ..p.clone() };
    q.renew_identity();
    q
}

fn a_float() -> (Box<Float>, Run) {
    let doc = read("FX-STORIES.docx");
    doc.sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .find_map(|b| match b {
            Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                Inline::Float(f, r) if f.text_box.is_some() => Some((f.clone(), r.clone())),
                _ => None,
            }),
            _ => None,
        })
        .expect("a float with a text box")
}

#[test]
fn joins_and_block_edits_in_the_sub_template_flow_give_layout_their_block_delta() {
    let mut doc = read("FX-SUBTEMPLATES.docx");
    let mut idx = DocIndex::build(&doc);
    idx.flow = Some(0);
    let c = Container::Story(StoryId::BuildingBlock(0));
    let body = &doc.glossary.as_ref().expect("glossary").parts[0].body;
    let Block::Para(first) = &body[0] else { panic!("a paragraph") };
    let mut cur = ParaId(first.id);
    for _ in 0..4 {
        let p = idx.para(&doc, cur).expect("para").clone();
        let second = blank_para_like(&p);
        let sid = ParaId(second.id);
        Step::Split { para: cur, at: Unit(0), second, kept: Vec::new() }.apply(&mut doc, &mut idx).expect("split");
        cur = sid;
    }
    assert!(idx.blocks(&doc, c).expect("blocks").len() >= 5);
    let applied = Step::Blocks { container: c, range: 1..3, with: Vec::new() }.apply(&mut doc, &mut idx).expect("blocks");
    assert_eq!(applied.hint, Some(Change { section: 0, first: 1, old_end: 3, new_end: 1, row: None }), "Blocks hint");
    let (a, b) = {
        let bl = idx.blocks(&doc, c).expect("blocks");
        let (Block::Para(a), Block::Para(b)) = (&bl[0], &bl[1]) else { panic!("paragraphs") };
        (ParaId(a.id), ParaId(b.id))
    };
    let applied = Step::Join { first: a, second: b }.apply(&mut doc, &mut idx).expect("join");
    assert_eq!(applied.hint, Some(Change { section: 0, first: 0, old_end: 2, new_end: 1, row: None }), "Join hint");
}

#[test]
fn a_style_edit_and_its_undo_restyle_the_line_numbers() {
    let mut e = open("FX-STYLES.docx");
    e.complete(fonts());
    let want = |e: &Editor| e.document().resolver.as_ref().map(|r| crate::docx::line_number_run(r));
    let same = |a: &Option<crate::docx::Run>, b: &Option<crate::docx::Run>| match (a, b) {
        (Some(a), Some(b)) => a.layout_eq(b),
        (a, b) => a.is_none() && b.is_none(),
    };
    assert!(same(&e.document().line_number_run, &want(&e)));
    let raw = "<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"90\"/></w:rPr></w:rPrDefault></w:docDefaults>".to_string();
    let out = e.edit_styles("Set Defaults", |s| s.set_defaults_xml(raw), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert!(same(&e.document().line_number_run, &want(&e)), "after the edit");
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(same(&e.document().line_number_run, &want(&e)), "after undo");
}

/// FX-TEXT with its first paragraph an empty run carrying the formatting before it, as a split leaves one.
fn with_a_carrier() -> Document {
    let mut doc = read("FX-TEXT.docx");
    let Some(Block::Para(p)) = doc.sections[0].blocks.first_mut() else { panic!("a paragraph") };
    let r = p.mark.clone();
    p.inlines = vec![Inline::Text(String::new(), r)];
    doc
}

#[test]
fn a_bookmark_in_a_new_paragraph_keeps_its_empty_run() {
    use crate::editing::commands::{Dir, MoveBy};
    let mut e = Editor::open(&super::blank_with("Hello"), fonts()).expect("opens");
    e.execute(Command::SelectAll, fonts());
    e.execute(Command::SetBold(true), fonts());
    e.execute(super::mv(MoveBy::Story, Dir::Forward, false), fonts());
    let edits = [Command::InsertParagraph, Command::InsertBookmark("b".into()), Command::InsertText("x".into())];
    for c in edits.clone() {
        e.execute(c, fonts());
    }
    let x_bold = |e: &Editor| {
        let Some(Block::Para(p)) = e.document().sections[0].blocks.get(1) else { panic!("a second paragraph") };
        p.inlines.iter().find_map(|i| if let Inline::Text(t, r) = i { (t == "x").then_some(r.bold) } else { None })
    };
    assert_eq!(x_bold(&e), Some(true), "typed after the bookmark");
    let done = e.document().sections[0].blocks.len();
    for _ in &edits {
        e.execute(Command::Undo(Default::default()), fonts());
    }
    assert_eq!(e.document().sections[0].blocks.len(), 1, "undone");
    for _ in &edits {
        e.execute(Command::Redo(Default::default()), fonts());
    }
    assert_eq!(e.document().sections[0].blocks.len(), done);
    assert_eq!(x_bold(&e), Some(true), "after undo and redo");
}

#[test]
fn undoing_typing_into_an_empty_paragraph_restores_its_empty_run() {
    let mut doc = with_a_carrier();
    let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
    let (id, original, r) = (ParaId(p.id), p.inlines.clone(), p.mark.clone());
    let mut idx = DocIndex::build(&doc);
    let applied = Step::Text { para: id, range: Unit(0)..Unit(0), with: vec![Inline::Text("x".into(), r)], edges: Default::default() }
        .apply(&mut doc, &mut idx)
        .expect("insert");
    let redo = applied.inverse.apply(&mut doc, &mut idx).expect("inverse");
    assert_eq!(idx.para(&doc, id).expect("para").inlines, original);
    redo.inverse.apply(&mut doc, &mut idx).expect("redo");
    assert!(matches!(&idx.para(&doc, id).expect("para").inlines[..], [Inline::Text(t, _)] if t == "x"));
}

#[test]
fn undoing_a_join_into_an_empty_paragraph_restores_its_empty_run() {
    let doc0 = with_a_carrier();
    assert!(matches!(doc0.sections[0].blocks.get(1), Some(Block::Para(q)) if para_len(q).0 > 0), "a text paragraph follows");
    let mut doc = doc0.clone();
    let mut idx = DocIndex::build(&doc);
    let (Block::Para(a), Block::Para(b)) = (&doc.sections[0].blocks[0], &doc.sections[0].blocks[1]) else { panic!("paras") };
    let (a, b) = (ParaId(a.id), ParaId(b.id));
    let applied = Step::Join { first: a, second: b }.apply(&mut doc, &mut idx).expect("join");
    applied.inverse.apply(&mut doc, &mut idx).expect("inverse");
    let (Block::Para(now), Block::Para(was)) = (&doc.sections[0].blocks[0], &doc0.sections[0].blocks[0]) else { panic!("para") };
    assert_eq!(now.inlines, was.inlines);
}

#[test]
fn controls_in_text_boxes_in_building_blocks_are_listed() {
    let mut doc = read("FX-SUBTEMPLATES.docx");
    let (mut f, run) = a_float();
    f.id = crate::docx::next_para_id();
    let c = crate::docx::ControlId(crate::docx::next_para_id());
    let tb = f.text_box.as_mut().expect("box");
    let Some(Block::Para(bp)) = tb.blocks.iter_mut().find(|b| matches!(b, Block::Para(_))) else { panic!("a paragraph") };
    bp.inlines = vec![Inline::ControlStart(c), Inline::ControlEnd(c)];
    let fid = FloatId(f.id);
    let g = doc.glossary.as_mut().expect("glossary");
    let Some(Block::Para(host)) = g.parts[0].body.iter_mut().find(|b| matches!(b, Block::Para(_))) else { panic!("a paragraph") };
    host.inlines.push(Inline::Float(f, run));
    let idx = DocIndex::build(&doc);
    let all = super::super::elements::enumerate(&doc, &idx, &super::super::elements::Filter::default());
    let found = all.iter().find(|x| x.id == crate::docx::ElementId::Control(c));
    assert_eq!(found.map(|x| x.story), Some(StoryId::TextBox(fid)));
}

#[test]
fn a_table_in_a_text_box_in_a_deep_cell_is_refused_for_its_depth() {
    use crate::docx::Table;
    let doc = read("FX-NEST-2.docx");
    let Some(Block::Table(outer)) = doc.sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))).cloned() else { panic!("a table") };
    let para = |text: &str| {
        let Some(Block::Para(mut p)) = outer.rows[0].cells[0].blocks.first().cloned() else { panic!("a paragraph") };
        p.renew_identity();
        let run = p.mark.clone();
        p.inlines = vec![Inline::Text(text.to_string(), run)];
        p
    };
    let one_cell = |content: Vec<Block>| -> Table {
        let mut t = outer.clone();
        t.rows.truncate(1);
        t.rows[0].cells.truncate(1);
        t.rows[0].cells[0].span = 1;
        t.rows[0].cells[0].blocks = content;
        t.renew_identity();
        t
    };
    let (mut f, run) = a_float();
    f.id = crate::docx::next_para_id();
    let boxed = para("boxed");
    let boxed_id = boxed.id;
    f.text_box.as_mut().expect("box").blocks = vec![Block::Para(boxed)];
    let fid = FloatId(f.id);
    let reopens = |d: &Document| crate::docx::write::save(d).ok().is_some_and(|b| Editor::open(&b, fonts()).is_ok());
    let build = |depth: usize, f: &crate::docx::Float| {
        let mut inner = para("deep");
        inner.inlines.push(Inline::Float(Box::new(f.clone()), run.clone()));
        let mut t = one_cell(vec![Block::Para(inner)]);
        for _ in 1..depth {
            t = one_cell(vec![Block::Para(para("l")), Block::Table(t), Block::Para(para(""))]);
        }
        let mut d = doc.clone();
        d.sections[0].blocks = vec![Block::Para(para("Before.")), Block::Table(t), Block::Para(para("After."))];
        d
    };
    let depth = (1..=29).rev().find(|&n| reopens(&build(n, &f))).expect("some depth reopens");
    assert!(depth > 1, "a shallow and a deep case: {depth}");
    let insert = Command::InsertTable(crate::api::command::TableSize { rows: 1, columns: 1 });
    for (depth, accepts) in [(1, true), (depth, false)] {
        let mut e = Editor::with_document(build(depth, &f), fonts());
        e.complete(fonts());
        e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::TextBox(fid), ParaId(boxed_id), Unit(0))));
        let out = e.execute(insert.clone(), fonts());
        if accepts {
            assert!(matches!(out, Outcome::Changed(_)), "depth {depth}: {out:?}");
        } else {
            assert!(matches!(out, Outcome::Unchanged(why) if why.starts_with("tables nested deeper")), "depth {depth}: {out:?}");
        }
        assert!(reopens(e.document()), "depth {depth}: the document reopens");
    }
}

#[test]
fn integrity_checks_tables_in_text_boxes() {
    let (mut f, run) = a_float();
    f.id = crate::docx::next_para_id();
    let Some(Block::Table(mut table)) = read("FX-TBL-BASIC.docx").sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))).cloned() else {
        panic!("a table")
    };
    table.rows[0].cells[0].blocks.clear();
    f.text_box.as_mut().expect("box").blocks.push(Block::Table(table));
    let e = open("FX-TEXT.docx");
    let mut doc = e.document().clone();
    let Some(Block::Para(host)) = doc.sections[0].blocks.iter_mut().find(|b| matches!(b, Block::Para(_))) else { panic!("a paragraph") };
    host.inlines.push(Inline::Float(f, run));
    let found = integrity::check(&doc, &DocIndex::build(&doc), &e.selection());
    assert!(found.iter().any(|v| v.invariant.starts_with("1 ")), "a cell without a final paragraph in a text box: {found:?}");
}

#[test]
fn integrity_finds_repeated_ids_in_an_inline_shapes_text() {
    let mut doc = read("FX-OBJ.docx");
    let (mut pc, run) = doc
        .sections
        .iter()
        .flat_map(|s| &s.blocks)
        .find_map(|b| match b {
            Block::Para(p) => p.inlines.iter().find_map(|i| if let Inline::Picture(pc, r) = i { Some((pc.clone(), r.clone())) } else { None }),
            Block::Table(_) => None,
        })
        .expect("an inline picture");
    let Some(Block::Para(first)) = doc.sections[0].blocks.iter().find(|b| matches!(b, Block::Para(_))).cloned() else { panic!("para") };
    let dup = Paragraph { inlines: Vec::new(), ..first };
    let tb = crate::docx::TextBox { blocks: vec![Block::Para(dup)], insets: [0.0; 4], outline: 0.0, auto_fit: false, v_anchor: 0, flow: 0 };
    pc.text = Some(Box::new(crate::docx::InlineText { id: crate::docx::next_para_id(), text_box: tb }));
    let Some(Block::Para(host)) = doc.sections[0].blocks.iter_mut().find(|b| matches!(b, Block::Para(_))) else { panic!("para") };
    host.inlines.push(Inline::Picture(pc, run));
    assert!(!integrity::duplicate_ids(&doc).is_empty());
}

#[test]
fn inspect_names_header_and_footer_stories_as_the_index_does() {
    let mut e = open("FX-STORIES.docx");
    e.complete(fonts());
    let (name, removed) = (super::parts_named(&e, "word/header2.xml")[0], super::parts_named(&e, "word/footer1.xml")[0]);
    e.doc.stories.remove(&removed);
    let story = e.index().header_footer(name).expect("indexed");
    let Some(Block::Para(p)) = e.document().stories[&name].first() else { panic!("para") };
    let para = ParaId(p.id);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(story, para, Unit(0))));
    let st = inspect(&mut e, "caret,dump", true);
    let idx = st["caret"]["para"].as_u64().or_else(|| st["selection"]["focus"]["para"].as_u64());
    assert!(idx.is_some(), "{st}");
    let dumped = st["dump"]["stories"]
        .as_array()
        .expect("stories")
        .iter()
        .find(|s| s["paragraphs"].as_array().is_some_and(|ps| ps.iter().any(|p| p["index"].as_u64() == idx)))
        .map(|s| s["story"].clone());
    assert_eq!(Some(st["caret"]["story"].clone()), dumped);
}
