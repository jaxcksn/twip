//! Sub-templates

use super::super::Editor;
use super::super::commands::Command;
use super::super::elements;
use super::super::position::StoryId;
use super::{fonts, open};
use crate::document::paragraph::{ListId, ListRef};
use crate::docx::{Block, Inline};
use crate::layout::{ParaId, Unit};

fn fixture() -> Editor {
    open("FX-SUBTEMPLATES.docx")
}

fn body_para(e: &Editor, part: usize, i: usize) -> ParaId {
    let blocks = &e.document().glossary.as_ref().expect("a glossary").parts[part].body;
    let paras: Vec<u64> = blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p.id) } else { None }).collect();
    ParaId(paras[i])
}

fn text_of(e: &Editor, para: ParaId) -> String {
    let p = e.index().para(e.document(), para).expect("the paragraph");
    p.inlines.iter().filter_map(|i| if let Inline::Text(s, _) = i { Some(s.as_str()) } else { None }).collect()
}

fn type_text(e: &mut Editor, s: &str) {
    for c in s.chars() {
        e.execute(Command::InsertText(c.to_string()), fonts());
    }
}

#[test]
fn a_building_block_body_is_a_story_that_edits_undoes_and_redoes() {
    let mut e = fixture();
    let p = body_para(&e, 0, 0);
    e.select_range(p, Unit(0), Unit(0), fonts());
    assert_eq!(e.selection().focus().map(|f| f.story), Some(StoryId::BuildingBlock(0)));
    type_text(&mut e, "New ");
    assert_eq!(text_of(&e, p), "New Violation notice");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(text_of(&e, p), "Violation notice");
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(text_of(&e, p), "New Violation notice");
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    let (again, _) = crate::docx::read_editable(&saved).expect("reads");
    let Block::Para(first) = &again.glossary.as_ref().unwrap().parts[0].body[0] else { panic!("a paragraph") };
    assert!(first.inlines.iter().any(|i| matches!(i, Inline::Text(s, _) if s.starts_with("New "))));
}

#[test]
fn elements_in_bodies_are_listed_after_the_documents() {
    let e = fixture();
    let all = elements::enumerate(e.document(), e.index(), &elements::Filter::default());
    let fee = all.iter().find(|x| x.code.as_deref().is_some_and(|c| c.contains("MERGEFIELD Fee "))).expect("Violation's field");
    assert_eq!(fee.story, StoryId::BuildingBlock(0));
    let officer = all.iter().find(|x| x.props.as_ref().is_some_and(|p| p.tag.as_deref() == Some("Officer/Name"))).expect("its control");
    assert_eq!(officer.story, StoryId::BuildingBlock(0));
    let last_body = all.iter().rposition(|x| !matches!(x.story, StoryId::BuildingBlock(_))).expect("template elements");
    assert!(all.iter().position(|x| matches!(x.story, StoryId::BuildingBlock(_))).unwrap() > last_body);
}

fn first_reference_value(e: &Editor) -> (Option<crate::docx::FieldKind>, Option<String>) {
    let crate::docx::Block::Para(p) = &e.document().sections[0].blocks[1] else { panic!("the first reference's paragraph") };
    match &p.inlines.iter().find(|i| matches!(i, Inline::FieldBegin { .. })) {
        Some(Inline::FieldBegin { kind, value, .. }) => (kind.clone(), value.clone()),
        _ => panic!("a field"),
    }
}

fn references(e: &Editor) -> Vec<(String, super::super::sub_templates::Resolution, bool, StoryId)> {
    let all = elements::enumerate(e.document(), e.index(), &elements::Filter::default());
    all.iter()
        .filter_map(|x| match x.id {
            crate::docx::ElementId::Field(f) => {
                e.references().by_field.get(&f).map(|r| (r.name.clone(), r.resolution.clone(), r.row.is_some(), x.story))
            }
            crate::docx::ElementId::Control(_) => None,
        })
        .collect()
}

#[test]
fn rules_make_references_and_stop_the_bookmark_reading() {
    use super::super::sub_templates::{Resolution as R, rules};
    use crate::docx::FieldKind;
    let mut e = fixture();
    assert_eq!(first_reference_value(&e).1.as_deref(), Some("Error! Bookmark not defined."));
    assert!(e.references().by_field.is_empty());
    e.execute(rules("SUBTEMPLATE", "Sub-templates"), fonts());
    assert_eq!(first_reference_value(&e), (Some(FieldKind::SubTemplate), None));
    let refs = references(&e);
    let find =
        |name: &str, story: fn(&StoryId) -> bool| refs.iter().find(|r| r.0 == name && story(&r.3)).cloned().unwrap_or_else(|| panic!("{name}"));
    let body = |s: &StoryId| *s == StoryId::Body;
    assert_eq!(find("Violation", body).1, R::Resolved);
    assert_eq!(find("Witness", body).1, R::Missing);
    assert_eq!(find("violation", body).1, R::NearMatch("Violation".into()));
    assert!(find("FeeRow", body).2, "FeeRow's reference is alone in its row");
    assert_eq!(find("Violation", |s| matches!(s, StoryId::HeaderFooter(_))).1, R::NotInBody);
    let shared = refs.iter().filter(|r| r.0 == "Violation" && r.3 == StoryId::Body).filter(|r| r.1 == R::NotAlone).count();
    assert_eq!(shared, 1, "the reference sharing its paragraph");
    assert_eq!(find("Cycle-B", |s| matches!(s, StoryId::BuildingBlock(_))).1, R::Resolved);
    e.execute(rules("SUBTEMPLATE", "General"), fonts());
    assert_eq!(references(&e).iter().find(|r| r.0 == "Violation").map(|r| r.1.clone()), Some(R::Missing));
    e.execute(Command::SetSubTemplateRules(crate::api::command::SubTemplateRules::default()), fonts());
    assert!(e.references().by_field.is_empty());
    assert_eq!(first_reference_value(&e).1.as_deref(), Some("Error! Bookmark not defined."));
}

#[test]
fn typing_beside_a_reference_makes_it_not_alone() {
    use super::super::sub_templates::{Resolution as R, rules};
    let mut e = fixture();
    e.execute(rules("SUBTEMPLATE", "Sub-templates"), fonts());
    let crate::docx::Block::Para(p) = &e.document().sections[0].blocks[1] else { panic!("a paragraph") };
    let para = ParaId(p.id);
    e.select_range(para, Unit(0), Unit(0), fonts());
    type_text(&mut e, "x");
    let r = e.references().by_field.values().find(|r| r.at == para).expect("the reference");
    assert_eq!(r.resolution, R::NotAlone);
    e.execute(Command::Undo(Default::default()), fonts());
    let r = e.references().by_field.values().find(|r| r.at == para).expect("the reference");
    assert_eq!(r.resolution, R::Resolved);
}

#[test]
fn a_reference_typed_by_hand_is_a_reference() {
    use super::super::sub_templates::{Resolution as R, rules};
    let mut e = fixture();
    e.execute(rules("SUBTEMPLATE", "Sub-templates"), fonts());
    let p = ParaId(match &e.document().sections[0].blocks[2] {
        crate::docx::Block::Para(p) => p.id,
        _ => panic!("a paragraph"),
    });
    e.select_range(p, Unit(0), Unit(20), fonts());
    e.execute(Command::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Backward, unit: None }), fonts());
    e.execute(
        Command::InsertField(crate::api::command::InsertField { code: "SUBTEMPLATE \"Appeals notice\"".into(), display: None, show_code: None }),
        fonts(),
    );
    let r = e.references().by_field.values().find(|r| r.at == p).expect("a reference");
    assert_eq!((r.name.as_str(), &r.resolution), ("Appeals notice", &R::Resolved));
    let crate::docx::Block::Para(para) = &e.document().sections[0].blocks[2] else { panic!("a paragraph") };
    assert!(para.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { kind: Some(crate::docx::FieldKind::SubTemplate), .. })));
}

#[test]
fn a_reference_to_the_sub_template_being_edited_can_be_edited_over() {
    use super::super::sub_templates::Resolution as R;
    let (mut e, _) = expanded();
    let name = Command::EditSubTemplate(crate::api::command::SubTemplateName { name: "Appeals notice".into(), category: None });
    e.execute(name, fonts());
    assert_eq!(e.editing_sub_template(), Some(2));
    let p = body_para(&e, 2, 0);
    let len = super::super::step::para_len(e.index().para(e.document(), p).expect("the paragraph"));
    e.select_range(p, Unit(0), len, fonts());
    e.execute(Command::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Backward, unit: None }), fonts());
    e.execute(
        Command::InsertField(crate::api::command::InsertField { code: "SUBTEMPLATE \"Appeals notice\"".into(), display: None, show_code: None }),
        fonts(),
    );
    let r = e.references().by_field.values().find(|r| r.at == p).expect("a reference");
    assert_eq!(r.resolution, R::Resolved, "nothing in the body leads to it");
    let len = super::super::step::para_len(e.index().para(e.document(), p).expect("the paragraph"));
    let at = |u| super::super::position::Position::text(StoryId::BuildingBlock(2), p, u);
    assert_eq!(e.expanded_in(&at(Unit(0)), &at(len)), None, "it doesn't expand in itself");
}

fn body_block(e: &Editor, s: usize, i: usize) -> ParaId {
    match &e.document().sections[s].blocks[i] {
        Block::Para(p) => ParaId(p.id),
        Block::Table(_) => panic!("a table"),
    }
}

fn line_of(e: &mut Editor, para: ParaId) -> (usize, f64, f64) {
    e.page_geometries().into_iter().find_map(|(pg, g)| g.lines.iter().find(|l| l.shows(para)).map(|l| (pg, l.top, l.bottom))).expect("a line")
}

#[test]
fn a_resolved_reference_shows_its_body_in_place() {
    use super::super::sub_templates::rules;
    let mut e = fixture();
    e.complete(fonts());
    let (reference, after) = (body_block(&e, 0, 1), body_block(&e, 0, 2));
    let (_, ref_top, ref_bottom) = line_of(&mut e, reference);
    let (_, after_top, _) = line_of(&mut e, after);
    e.execute(rules("SUBTEMPLATE", "Sub-templates"), fonts());
    e.complete(fonts());
    let (_, top, bottom) = line_of(&mut e, reference);
    assert_eq!(top, ref_top);
    let grown = (bottom - top) - (ref_bottom - ref_top);
    assert!(grown > 60.0, "the reference's line holds Violation's body ({grown} more)");
    let (_, now_after, _) = line_of(&mut e, after);
    assert!((now_after - after_top - grown).abs() < 1.0, "the next paragraph moves down by as much");
    let shown = e.expansions.clone().expect("expansions");
    assert!((shown.paras[&reference].size.0 - 468.0).abs() < 0.5);
    let landscape = body_block(&e, 1, 0);
    assert!((shown.paras[&landscape].size.0 - 648.0).abs() < 0.5);
    assert_eq!(shown.paras.len(), 3, "Violation twice and Cycle-A");
    assert_eq!(shown.rows.len(), 1);
    let text = crate::layout::dump::page_text(&e.snapshot(), e.document());
    assert!(text.contains("«Name»") && text.contains("«Amount»"), "FeeRow's fields are drawn in the fees table");
    let ex = &shown.paras[&reference];
    assert!(!ex.content.is_empty());
    let cycle = shown.paras.values().find(|x| x.part == 3).expect("Cycle-A");
    assert!(cycle.size.1 > 20.0);
}

#[test]
fn new_labels_redraw_the_shown_bodies() {
    use std::io::Read;
    // The fixture's sub-template fields, with their results emptied so that design mode labels them.
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx")).unwrap();
    let mut xml = String::new();
    zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).unwrap().by_name("word/glossary/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    for r in ["«Fee»", "«Name»", "«Amount»"] {
        xml = xml.replace(&format!(">{r}</w:t>"), "></w:t>");
    }
    let bytes =
        crate::docx::write::repackage(&bytes, &[("word/glossary/document.xml".into(), xml.into_bytes())], zip::CompressionMethod::Deflated).unwrap();
    let mut e = Editor::open(&bytes, fonts()).unwrap();
    e.set_mode(super::super::Mode::Design);
    e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    let drawn = |e: &mut Editor| {
        e.complete(fonts());
        crate::layout::dump::page_text(&e.snapshot(), e.document())
    };
    let first = drawn(&mut e);
    assert!(first.contains("«Fee»") && first.contains("«Fee.Name»"), "the first labels");
    let labels = [("MERGEFIELD Fee", "Fee label"), ("MERGEFIELD Fee.Name", "Name label")].map(|(c, l)| (c.to_string(), l.to_string()));
    e.execute(Command::SetFieldLabels(crate::api::command::FieldLabels { labels: labels.into_iter().collect(), merge: None }), fonts());
    let text = drawn(&mut e);
    assert!(text.contains("Fee label"), "in a shown paragraph body");
    assert!(text.contains("Name label"), "in a shown row");
}

fn click_line(e: &mut Editor, para: ParaId, clicks: u8) {
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    let (pg, g) = e.page_geometries().into_iter().find(|(_, g)| g.lines.iter().any(|l| l.shows(para))).expect("a page");
    let l = g.lines.iter().find(|l| l.shows(para)).expect("a line");
    let (x, y) = e.stack().to_doc(pg, l.stops[0].x + 40.0, (l.top + l.bottom) / 2.0).expect("on the stack");
    for c in 1..=clicks {
        e.pointer_input(PointerKind::Down, x, y, c, Mods::default(), fonts());
        e.pointer_input(PointerKind::Up, x, y, c, Mods::default(), fonts());
    }
}

fn expanded() -> (Editor, ParaId) {
    let mut e = fixture();
    e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    e.complete(fonts());
    let reference = body_block(&e, 0, 1);
    (e, reference)
}

fn has_field(e: &Editor, para: ParaId) -> bool {
    e.index().para(e.document(), para).is_some_and(|p| p.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { .. })))
}

#[test]
fn a_click_on_a_shown_body_selects_its_reference_and_typing_is_refused() {
    use super::super::input::Notice;
    let (mut e, reference) = expanded();
    click_line(&mut e, reference, 1);
    let sel = e.selection();
    assert!(!sel.is_collapsed(), "the reference field is selected");
    assert!(sel.focus().is_some_and(|f| f.para_offset().is_some_and(|(p, _)| p == reference)));
    e.take_notices();
    let before = e.document().sections[0].blocks[1].clone();
    type_text(&mut e, "x");
    assert_eq!(e.document().sections[0].blocks[1], before, "nothing changed");
    let refused: Vec<Notice> = e.take_notices().into_iter().filter(|n| matches!(n, Notice::Refused { .. })).collect();
    assert_eq!(refused.len(), 1);
    assert!(matches!(&refused[0], Notice::Refused { sub_template: Some(n), element: Some(_) } if n == "Violation"));
    e.execute(Command::Copy, fonts());
    let copied = e.clipboard().expect("copied");
    let text: String = format!("{copied:?}");
    assert!(text.contains("FieldBegin") && !text.contains("Violation notice"));
    e.execute(Command::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Backward, unit: None }), fonts());
    assert!(!has_field(&e, reference));
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(has_field(&e, reference));
}

#[test]
fn typing_over_paragraphs_around_a_shown_body_is_refused() {
    let (mut e, reference) = expanded();
    let (before, after) = (body_block(&e, 0, 0), body_block(&e, 0, 2));
    let end = e.index().para(e.document(), after).map(|p| p.inlines.iter().map(Inline::units).sum::<usize>() as u32).expect("a paragraph");
    let at = |p, u| super::super::position::Position::text(StoryId::Body, p, Unit(u));
    let selection = super::super::selection::Selection::range(at(before, 0), at(after, end), super::super::selection::Granularity::Char);
    e.set_caret_for_benchmark(selection);
    let blocks = e.document().sections[0].blocks.clone();
    type_text(&mut e, "x");
    assert_eq!(e.document().sections[0].blocks, blocks, "nothing changed");
    assert!(has_field(&e, reference));
}

#[test]
fn typing_into_a_locked_control_says_so() {
    use super::super::input::Notice;
    let mut e = open("FX-MERGE-CONTROLS.docx");
    let all = elements::enumerate(e.document(), e.index(), &elements::Filter::default());
    let locked = all.iter().find(|x| x.props.as_ref().is_some_and(|p| p.tag.as_deref() == Some("Lock/Content"))).expect("the control");
    let para = locked.at.expect("an inline control");
    let text = text_of(&e, para);
    let at = text.find("cannot").expect("its text") as u32;
    e.select_range(para, Unit(at + 2), Unit(at + 2), fonts());
    e.take_notices();
    type_text(&mut e, "x");
    assert!(e.take_notices().iter().any(|n| matches!(n, Notice::Refused { sub_template: None, .. })));
}

#[test]
fn incremental_layout_with_shown_bodies_equals_a_full_one() {
    let (mut e, reference) = expanded();
    let after = body_block(&e, 0, 2);
    let violation = body_para(&e, 0, 1);
    let check = |e: &mut Editor, step: &str| {
        e.complete(fonts());
        let shown = e.expansions.clone().unwrap_or_default();
        let ours = crate::layout::dump::page_text(&e.snapshot(), e.document());
        let fresh = crate::layout::dump::page_text(&crate::layout::document::layout_expanded(e.document(), fonts(), &shown), e.document());
        assert!(ours == fresh, "{step}: incremental layout differs from a full one");
    };
    check(&mut e, "expanded");
    for (i, (para, word)) in [(after, "more "), (violation, "long words that wrap the line "), (after, "x")].into_iter().enumerate() {
        e.select_range(para, Unit(0), Unit(0), fonts());
        for _ in 0..4 {
            type_text(&mut e, word);
        }
        check(&mut e, &format!("edit {i}"));
    }
    e.execute(Command::SetSubTemplateView(crate::api::command::SubTemplateView { ids: None, expanded: false }), fonts());
    check(&mut e, "collapsed");
    assert!(e.expansions.as_ref().is_some_and(|x| x.paras.is_empty()));
    e.execute(Command::SetSubTemplateView(crate::api::command::SubTemplateView { ids: None, expanded: true }), fonts());
    check(&mut e, "expanded again");
    let _ = reference;
}

fn enabled(e: &mut Editor, id: crate::api::command::CommandId) -> bool {
    let f = e.format_state();
    e.command_states(f.as_ref(), super::super::state::StateFlags { read_only: false })[&id].enabled
}

fn shown_stories(e: &mut Editor) -> Vec<StoryId> {
    let (_, g) = e.page_geometries().into_iter().next().expect("a page");
    let mut v: Vec<StoryId> =
        g.lines.iter().filter_map(|l| l.para()).filter_map(|p| e.index().para_slot(p).map(|s| e.index().story(s.container))).collect();
    v.dedup();
    v
}

#[test]
fn a_double_click_edits_the_sub_template_in_the_same_editor() {
    use crate::api::command::CommandId as C;
    let (mut e, reference) = expanded();
    assert!(!enabled(&mut e, C::CloseSubTemplate));
    let shown_before = e.expansions.as_ref().expect("expansions").paras[&reference].clone();
    click_line(&mut e, reference, 1);
    let before = e.selection();
    click_line(&mut e, reference, 2);
    assert_eq!(e.editing_sub_template(), Some(0));
    e.complete(fonts());
    assert_eq!(shown_stories(&mut e), [StoryId::BuildingBlock(0)], "the pages show only Violation's body");
    assert_eq!(e.snapshot().pages()[0].w, 612.0, "Letter, as the reference's section");
    for id in [C::EditHeader, C::SetMargins, C::InsertPageNumber, C::InsertPageBreak] {
        assert!(!enabled(&mut e, id), "{id:?}");
    }
    assert!(enabled(&mut e, C::CloseSubTemplate));
    let first = body_para(&e, 0, 0);
    e.select_range(first, Unit(0), Unit(0), fonts());
    type_text(&mut e, "x");
    assert_eq!(text_of(&e, first), "xViolation notice");
    e.execute(Command::CloseSubTemplate, fonts());
    assert_eq!(e.editing_sub_template(), None);
    assert_eq!(e.selection(), before);
    e.complete(fonts());
    let shown_after = e.expansions.as_ref().expect("expansions").paras[&reference].clone();
    assert!(!std::rc::Rc::ptr_eq(&shown_before.content, &shown_after.content), "the body was laid out again");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(e.editing_sub_template(), Some(0));
    assert_eq!(text_of(&e, first), "Violation notice");
    e.execute(Command::CloseSubTemplate, fonts());
    e.complete(fonts());
    assert_eq!(shown_stories(&mut e)[0], StoryId::Body);
}

#[test]
fn a_sub_template_entered_from_the_landscape_section_has_landscape_pages() {
    let (mut e, _) = expanded();
    let landscape = body_block(&e, 1, 0);
    click_line(&mut e, landscape, 2);
    assert_eq!(e.editing_sub_template(), Some(0));
    e.complete(fonts());
    assert_eq!(e.snapshot().pages()[0].w, 792.0);
}

#[test]
fn edit_sub_template_by_name_and_back() {
    let (mut e, _) = expanded();
    let name = |n: &str| Command::EditSubTemplate(crate::api::command::SubTemplateName { name: n.into(), category: None });
    assert!(matches!(e.execute(name("Appeals notice"), fonts()), super::super::commands::Outcome::Moved));
    assert_eq!(e.editing_sub_template(), Some(2));
    e.execute(name("Violation"), fonts());
    assert_eq!(e.editing_sub_template(), Some(0));
    assert!(matches!(e.execute(name("Nope"), fonts()), super::super::commands::Outcome::Refused(_)));
    assert_eq!(e.editing_sub_template(), Some(0));
    assert!(matches!(e.execute(name("Greeting"), fonts()), super::super::commands::Outcome::Refused(_)));
    e.execute(Command::CloseSubTemplate, fonts());
    assert_eq!(e.editing_sub_template(), None);
}

#[test]
fn find_searches_the_sub_template_being_edited_only() {
    let (mut e, _) = expanded();
    assert!(e.find("First count", true, false).is_none(), "the template doesn't search Violation's body");
    e.execute(Command::EditSubTemplate(crate::api::command::SubTemplateName { name: "Violation".into(), category: None }), fonts());
    assert!(e.find("First count", true, false).is_some());
    assert!(e.find("After the violation", true, false).is_none(), "nor the sub-template the template's");
}

fn create(e: &mut Editor, name: &str, from: Option<Vec<u8>>) -> super::super::commands::Outcome {
    e.execute(
        Command::CreateSubTemplate(crate::api::command::CreateSubTemplate {
            name: name.into(),
            category: None,
            from: from.map(crate::api::value::Bytes),
            copy_of: None,
        }),
        fonts(),
    )
}

fn part_names(e: &Editor) -> Vec<String> {
    e.document()
        .glossary
        .as_ref()
        .map_or(Vec::new(), |g| g.parts.iter().filter(|b| b.state != crate::docx::PartState::Removed).map(|b| b.name.clone()).collect())
}

fn codes(e: &Editor) -> Vec<String> {
    elements::enumerate(e.document(), e.index(), &elements::Filter::default()).into_iter().filter_map(|x| x.code).collect()
}

#[test]
fn sub_templates_are_created_renamed_and_removed_each_as_one_step() {
    use super::super::commands::Outcome;
    use super::super::sub_templates::Resolution as R;
    let (mut e, _) = expanded();
    assert!(matches!(create(&mut e, "Payment", None), Outcome::Changed(_)));
    let docx = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-TEXT.docx")).unwrap();
    assert!(matches!(create(&mut e, "Imported", Some(docx.clone())), Outcome::Changed(_)));
    assert!(matches!(create(&mut e, "Imported again", Some(docx)), Outcome::Changed(_)));
    assert!(matches!(create(&mut e, "Payment", None), Outcome::Refused(_)), "a duplicate name");
    assert!(part_names(&e).ends_with(&["Payment".into(), "Imported".into(), "Imported again".into()]));
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(!part_names(&e).contains(&"Imported again".into()));
    let rename =
        crate::api::command::RenameSubTemplate { name: "Violation".into(), to: "Citation".into(), category: None, update_references: Some(true) };
    assert!(matches!(e.execute(Command::RenameSubTemplate(rename), fonts()), Outcome::Changed(_)));
    assert!(part_names(&e).contains(&"Citation".into()));
    assert!(codes(&e).iter().any(|c| c.contains("SUBTEMPLATE Citation")));
    assert!(!codes(&e).iter().any(|c| c.contains("SUBTEMPLATE Violation")));
    let resolved = e.references().by_field.values().filter(|r| r.name == "Citation" && r.resolution == R::Resolved).count();
    assert!(resolved >= 2);
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(part_names(&e).contains(&"Violation".into()) && codes(&e).iter().any(|c| c.contains("SUBTEMPLATE Violation")));
    let remove = |n: &str| Command::RemoveSubTemplate(crate::api::command::SubTemplateName { name: n.into(), category: None });
    e.execute(remove("Cycle-B"), fonts());
    assert!(!part_names(&e).contains(&"Cycle-B".into()));
    assert!(e.references().by_field.values().any(|r| r.name == "Cycle-B" && r.resolution == R::Missing));
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(e.references().by_field.values().any(|r| r.name == "Cycle-B" && r.resolution == R::Resolved));
    e.execute(Command::EditSubTemplate(crate::api::command::SubTemplateName { name: "Appeals notice".into(), category: None }), fonts());
    e.execute(remove("Appeals notice"), fonts());
    assert_eq!(e.editing_sub_template(), None);
    assert!(!part_names(&e).contains(&"Appeals notice".into()));
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    let (again, _) = crate::docx::read_editable(&saved).expect("reads");
    let names: Vec<&str> = again.glossary.as_ref().unwrap().parts.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Violation", "FeeRow", "Cycle-A", "Cycle-B", "Greeting", "Payment", "Imported"]);
}

#[test]
fn the_first_sub_template_makes_the_glossary_part() {
    let mut e = open("FX-EMPTY.docx");
    e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    assert!(matches!(create(&mut e, "Notice", None), super::super::commands::Outcome::Changed(_)));
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    let z = zip::ZipArchive::new(std::io::Cursor::new(saved.clone())).expect("a package");
    assert!(z.file_names().any(|n| n.is_ok_and(|n| n == "word/glossary/document.xml")));
    let (again, _) = crate::docx::read_editable(&saved).expect("reads");
    let g = again.glossary.as_ref().expect("a glossary");
    assert_eq!((g.parts[0].name.as_str(), g.parts[0].category.as_str()), ("Notice", "Sub-templates"));
}

#[test]
fn cycles_and_depth_stop_with_a_warning() {
    use super::super::input::Notice;
    use super::super::sub_templates::Resolution as R;
    let mut e = fixture();
    e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    let warnings: Vec<Notice> = e.take_notices().into_iter().filter(|n| matches!(n, Notice::Warning(..))).collect();
    use crate::docx::diagnostics::WarningCode as W;
    assert!(warnings.iter().any(|n| matches!(n, Notice::Warning(W::SubTemplateCycle, m) if m.contains("Cycle-A"))));
    assert!(warnings.iter().any(|n| matches!(n, Notice::Warning(W::SubTemplateNearMatch, m) if m.contains("violation"))));
    let by = |name: &str| e.references().by_field.values().filter(|r| r.name == name).map(|r| r.resolution.clone()).collect::<Vec<_>>();
    assert!(by("Cycle-B").contains(&R::Resolved));
    assert!(by("Cycle-A").contains(&R::Cycle));
    e.execute(Command::SetSubTemplateView(crate::api::command::SubTemplateView { ids: None, expanded: false }), fonts());
    e.select_range(body_block(&e, 0, 2), Unit(0), Unit(0), fonts());
    type_text(&mut e, "x");
    assert!(!e.take_notices().iter().any(|n| matches!(n, Notice::Warning(..))));
    e.complete(fonts());
}

#[test]
fn a_chain_seventeen_deep_stops_at_the_seventeenth() {
    use super::super::sub_templates::Resolution as R;
    let mut e = fixture();
    e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    for i in 1..=18 {
        create(&mut e, &format!("L{i}"), None);
    }
    let insert = |e: &mut Editor, code: String| {
        e.execute(Command::InsertField(crate::api::command::InsertField { code, display: None, show_code: None }), fonts());
    };
    for i in 1..18 {
        e.execute(Command::EditSubTemplate(crate::api::command::SubTemplateName { name: format!("L{i}"), category: None }), fonts());
        insert(&mut e, format!("SUBTEMPLATE L{}", i + 1));
    }
    e.execute(Command::CloseSubTemplate, fonts());
    let p = body_block(&e, 0, 2);
    e.select_range(p, Unit(0), Unit(100), fonts());
    insert(&mut e, "SUBTEMPLATE L1".into());
    let at = |n: &str| e.references().by_field.values().find(|r| r.name == n).map(|r| r.resolution.clone());
    assert_eq!(at("L16"), Some(R::Resolved));
    assert_eq!(at("L17"), Some(R::TooDeep), "the 17th level");
    e.complete(fonts());
    assert!(e.expansions.as_ref().is_some_and(|x| x.paras.contains_key(&p)));
}

/// A body's paragraphs as their style, mark, list label run and runs (text and formatting), tables by their cells',
/// and its fields' codes and controls' tags and kinds. A label's number counts on across bodies sharing a list.
fn body_shape(e: &Editor, name: &str) -> (Vec<String>, Vec<(Option<String>, Option<String>)>) {
    let g = e.document().glossary.as_ref().expect("a glossary");
    let k = g.find(name, "Sub-templates", None).expect("the sub-template");
    fn blocks(list: &[Block], out: &mut Vec<String>) {
        for b in list {
            match b {
                Block::Para(p) => {
                    let runs: Vec<String> = p
                        .inlines
                        .iter()
                        .filter_map(|i| match i {
                            Inline::Text(s, r) => Some(format!("{s:?} {r:?}")),
                            Inline::Picture(pic, _) => Some(format!("picture {}x{}", pic.w, pic.h)),
                            _ => None,
                        })
                        .collect();
                    out.push(format!("{:?} {:?} {:?} {runs:?}", p.style, p.mark, p.list.as_ref().map(|l| &l.run)));
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| blocks(&c.blocks, out)),
            }
        }
    }
    let mut paras = Vec::new();
    blocks(&g.parts[k].body, &mut paras);
    let els = elements::enumerate(e.document(), e.index(), &elements::Filter::default())
        .into_iter()
        .filter(|x| x.story == StoryId::BuildingBlock(k as u32))
        .map(|x| (x.code, x.props.map(|p| format!("{:?} {:?}", p.kind, p.tag))))
        .collect();
    (paras, els)
}

#[test]
fn a_sub_template_exports_alone_and_copies_whole() {
    use super::super::commands::Outcome;
    let (mut e, _) = expanded();
    e.execute(Command::EditSubTemplate(crate::api::command::SubTemplateName { name: "Violation".into(), category: None }), fonts());
    let picture = serde_json::json!({ "kind": "picture", "tag": "Photo", "options": { "width": 90, "height": 60 } });
    e.execute(Command::InsertControl(serde_json::from_value(picture).unwrap()), fonts());
    e.execute(Command::CloseSubTemplate, fonts());
    let violation = body_shape(&e, "Violation");
    assert!(violation.1.iter().any(|(c, _)| c.as_deref().is_some_and(|c| c.contains("MERGEFIELD Fee"))), "{violation:?}");
    assert!(violation.0.iter().any(|p| p.contains("picture 90x60")));

    let docx = e.export_sub_template("Violation", None).expect("exports");
    let (alone, _) = crate::docx::read_editable(&docx).expect("reads");
    assert_eq!(alone.sections.len(), 1);
    assert!(alone.glossary.as_ref().is_none_or(|g| g.parts.iter().all(|b| b.state == crate::docx::PartState::Removed) || g.parts.is_empty()));
    assert!(alone.images.iter().any(|(_, i)| !i.is_empty()), "the picture's image");
    assert!(e.export_sub_template("Nope", None).is_err());

    let copy = |e: &mut Editor, name: &str, of: &str| {
        e.execute(
            Command::CreateSubTemplate(crate::api::command::CreateSubTemplate {
                name: name.into(),
                category: None,
                from: None,
                copy_of: Some(of.into()),
            }),
            fonts(),
        )
    };
    let depth = e.history().undo_depth();
    assert!(matches!(copy(&mut e, "Violation copy", "Violation"), Outcome::Changed(_)));
    assert_eq!(e.history().undo_depth(), depth + 1, "one undo step");
    assert_eq!(body_shape(&e, "Violation copy"), violation);
    assert!(matches!(create(&mut e, "Violation imported", Some(docx)), Outcome::Changed(_)));
    assert_eq!(body_shape(&e, "Violation imported"), violation, "the export imports back unchanged");
    assert!(matches!(copy(&mut e, "Cycle-A copy", "Cycle-A"), Outcome::Changed(_)));
    assert!(
        body_shape(&e, "Cycle-A copy").1.iter().any(|(c, _)| c.as_deref().is_some_and(|c| c.contains("SUBTEMPLATE Cycle-B"))),
        "nested reference"
    );
    assert!(matches!(copy(&mut e, "X", "Nope"), Outcome::Refused(_)));

    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(body_shape(&again, "Violation copy"), violation, "saved and read back, its controls with it");
}

#[test]
fn an_exported_sub_template_has_no_headers_or_footers() {
    let mut e = open("FX-STORIES.docx");
    e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    create(&mut e, "Notice", None);
    let main = |b: &[u8]| {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(b.to_vec())).expect("a package");
        std::io::read_to_string(z.by_name("word/document.xml").expect("the main part")).expect("text")
    };
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    assert!(main(&saved).contains("footerReference"), "the template has them");
    let alone = e.export_sub_template("Notice", None).expect("exports");
    let xml = main(&alone);
    assert!(!xml.contains("headerReference") && !xml.contains("footerReference"), "{xml}");
    crate::docx::read_editable(&alone).expect("reads");
}

fn body_texts(doc: &crate::docx::Document) -> Vec<String> {
    doc.sections
        .iter()
        .flat_map(|s| &s.blocks)
        .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
        .map(|p| p.inlines.iter().filter_map(|i| if let Inline::Text(s, _) = i { Some(s.as_str()) } else { None }).collect())
        .collect()
}

#[test]
fn the_body_is_replaced_from_a_docx_keeping_the_sub_templates() {
    use super::super::commands::Outcome;
    let replace = |e: &mut Editor, from: Vec<u8>| {
        e.execute(Command::ReplaceBody(crate::api::command::ReplaceBody { from: crate::api::value::Bytes(from) }), fonts())
    };
    let (mut e, _) = expanded();
    let ns = "urn:tylertech:court:forms:merge:1";
    let data = crate::api::command::WriteDataPart { namespace: ns.into(), item_id: None, xml: format!("<merge xmlns=\"{ns}\"/>") };
    e.execute(Command::WriteDataPart(data), fonts());
    let docx = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-TEXT.docx")).unwrap();
    let want = body_texts(&crate::docx::read_editable(&docx).unwrap().0);
    let (before, names, settings) = (body_texts(e.document()), part_names(&e), format!("{:?}", e.document().settings));
    assert_eq!(e.document().sections.len(), 2);
    e.execute(Command::EditSubTemplate(crate::api::command::SubTemplateName { name: "Violation".into(), category: None }), fonts());
    let depth = e.history().undo_depth();
    assert!(matches!(replace(&mut e, docx.clone()), Outcome::Changed(_)));
    assert_eq!(e.editing_sub_template(), None, "back on the template");
    assert_eq!(e.history().undo_depth(), depth + 1, "one undo step");
    assert_eq!((body_texts(e.document()), e.document().sections.len()), (want.clone(), 1));
    assert_eq!((part_names(&e), format!("{:?}", e.document().settings)), (names.clone(), settings));
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((body_texts(e.document()), e.document().sections.len()), (before, 2));
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(body_texts(e.document()), want);
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!((body_texts(again.document()), part_names(&again)), (want, names));
    assert!(again.document().data_parts.iter().any(|d| d.xml.contains(ns)), "the data part stays");
    assert!(matches!(replace(&mut e, b"not a docx".to_vec()), Outcome::Refused(_)));

    // An exported sub-template's body comes back as the template's, its fields and controls with it.
    let violation = body_shape(&e, "Violation");
    let alone = e.export_sub_template("Violation", None).expect("exports");
    replace(&mut e, alone);
    let template: Vec<_> = elements::enumerate(e.document(), e.index(), &elements::Filter::default())
        .into_iter()
        .filter(|x| x.story == StoryId::Body)
        .map(|x| (x.code, x.props.map(|p| format!("{:?} {:?}", p.kind, p.tag))))
        .collect();
    assert_eq!(template, violation.1);
}

#[test]
fn a_selected_shown_body_is_labelled_with_its_sub_template() {
    let (mut e, reference) = expanded();
    assert!(e.view(0..1).regions.is_empty(), "nothing selected, nothing hovered");
    click_line(&mut e, reference, 1);
    let regions = e.view(0..1).regions;
    let g = regions.iter().find(|g| g.title == "Violation").expect("the sub-template's label");
    let (_, line_top, line_bottom) = line_of(&mut e, reference);
    let (_, r) = g.rects[0];
    assert!(r[1] >= line_top - 0.5 && r[3] <= line_bottom + 0.5 && r[3] - r[1] > 60.0, "around the shown body: {r:?}");
}

#[test]
fn renaming_a_sub_template_rewrites_only_its_references_name_quoted() {
    use super::super::commands::Outcome;
    use super::super::sub_templates::Resolution as R;
    let (mut e, _) = expanded();
    let before = e.references().by_field.values().filter(|r| r.name == "Violation" && r.resolution == R::Resolved).count();
    assert!(before >= 2, "the fixture's references");
    let rename = |to: &str| {
        Command::RenameSubTemplate(crate::api::command::RenameSubTemplate {
            name: "Violation".into(),
            to: to.into(),
            category: None,
            update_references: Some(true),
        })
    };
    assert!(matches!(e.execute(rename("Violation notice"), fonts()), Outcome::Changed(_)));
    assert!(codes(&e).iter().any(|c| c.contains("SUBTEMPLATE \"Violation notice\"")), "{:?}", codes(&e));
    let after = e.references().by_field.values().filter(|r| r.name == "Violation notice" && r.resolution == R::Resolved).count();
    assert_eq!(after, before);
    e.execute(Command::Undo(Default::default()), fonts());
    // A backslash before the closing quote is escaped.
    assert!(matches!(e.execute(rename("ends in \\"), fonts()), Outcome::Changed(_)));
    assert!(codes(&e).iter().any(|c| c.contains("SUBTEMPLATE \"ends in \\\\\"")), "{:?}", codes(&e));
    let ended = e.references().by_field.values().filter(|r| r.name == "ends in \\" && r.resolution == R::Resolved).count();
    assert_eq!(ended, before);
}

#[test]
fn expanding_references_is_linear_in_the_references() {
    use std::io::Read;
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let src = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx")).expect("fixture");
    let reference = |name: &str| {
        format!(
            r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> SUBTEMPLATE {name} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#
        )
    };
    let part = |name: &str, body: &str| {
        format!(
            r#"<w:docPart><w:docPartPr><w:name w:val="{name}"/><w:category><w:name w:val="Sub-templates"/><w:gallery w:val="custom1"/></w:category><w:behaviors><w:behavior w:val="content"/></w:behaviors></w:docPartPr><w:docPartBody>{body}</w:docPartBody></w:docPart>"#
        )
    };
    // The body's references to a one-line part, and a part no reference shows holding `idle` references of its own.
    let time = |idle: usize| {
        let mut main = String::new();
        zip::ZipArchive::new(std::io::Cursor::new(&src)).unwrap().by_name("word/document.xml").unwrap().read_to_string(&mut main).unwrap();
        let sect = &main[main.rfind("<w:sectPr").expect("a section")..main.find("</w:body>").expect("a body")];
        let body: String = (0..400).map(|_| reference("Leaf")).collect();
        let doc = format!(
            r#"<w:document xmlns:w="{W}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}{sect}</w:body></w:document>"#
        );
        let unused: String = (0..idle).map(|_| reference("Nothing")).collect();
        let glossary = format!(
            r#"<w:glossaryDocument xmlns:w="{W}"><w:docParts>{}{}</w:docParts></w:glossaryDocument>"#,
            part("Leaf", "<w:p><w:r><w:t>leaf</w:t></w:r></w:p>"),
            part("Unused", &format!("<w:p/>{unused}"))
        );
        let parts = vec![("word/document.xml".to_string(), doc.into_bytes()), ("word/glossary/document.xml".to_string(), glossary.into_bytes())];
        let bytes = crate::docx::write::repackage(&src, &parts, zip::CompressionMethod::Deflated).expect("repackages");
        let mut e = Editor::open(&bytes, fonts()).expect("opens");
        e.complete(fonts());
        let t = std::time::Instant::now();
        e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
        e.complete(fonts());
        let took = t.elapsed().as_secs_f64();
        assert!(e.references().by_field.len() > idle, "the references");
        let shown: Vec<ParaId> = e
            .references()
            .by_field
            .values()
            .filter(|r| {
                r.resolution == super::super::sub_templates::Resolution::Resolved
                    && e.index().para_slot(r.at).is_some_and(|s| e.index().story(s.container) == StoryId::Body)
            })
            .map(|r| r.at)
            .collect();
        assert_eq!(shown.len(), 400, "the body's references resolve");
        let expansions = &e.expansions.as_ref().expect("expansions").paras;
        assert!(shown.iter().all(|p| expansions.contains_key(p)), "each shows its body");
        took
    };
    let (few, many) = (time(0), time(20_000));
    assert!(many < few * 3.0 + 0.5, "{few}s with no idle references, {many}s with 20000");
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name)).expect("fixture")
}

/// Each body paragraph's list format and level text, and each note reference's note text.
fn lists_and_notes(doc: &crate::docx::Document) -> (Vec<(String, String)>, Vec<String>) {
    let res = doc.resolver.as_ref().expect("styles");
    let mut lists = Vec::new();
    let mut notes = Vec::new();
    for b in doc.sections.iter().flat_map(|s| &s.blocks) {
        let Block::Para(p) = b else { continue };
        if let Some(ListRef { list: ListId(id), level: ilvl }) = p.style.num.filter(|n| n.list.0 != 0)
            && let Some(l) = res.cascade().level(ListId(id), ilvl)
        {
            lists.push((crate::docx::list_xml::format_name(l.format.as_ref()).to_string(), l.text.as_deref().unwrap_or_default().to_string()));
        }
        for i in &p.inlines {
            if let Inline::NoteRef { kind: crate::docx::NoteKind::Foot, id, .. } = i {
                let body = doc.footnotes.get(id).map(|b| {
                    b.iter()
                        .filter_map(|b| if let Block::Para(p) = b { Some(super::super::text::UnitText::of(p).text) } else { None })
                        .collect::<String>()
                });
                notes.push(body.unwrap_or_default());
            }
        }
    }
    (lists, notes)
}

#[test]
fn a_body_from_another_docx_brings_its_notes_comments_and_lists() {
    use super::super::commands::Outcome;
    let source = fixture_bytes("FX-A11Y-STRUCTURE.docx");
    let mut e = open("FX-STORIES.docx");
    e.complete(fonts());
    let out = e.execute(Command::ReplaceBody(crate::api::command::ReplaceBody { from: crate::api::value::Bytes(source.clone()) }), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let want = lists_and_notes(&crate::docx::read_editable(&source).expect("reads").0);
    assert!(!want.0.is_empty() && !want.1.is_empty(), "the source's lists and notes");
    assert_eq!(lists_and_notes(e.document()), want);
    let (source_doc, _) = crate::docx::read_editable(&source).expect("reads");
    let comments = comments_and_anchors(&source_doc);
    assert!(comments.iter().any(|c| !c.1.is_empty()), "the source's anchored comment: {comments:?}");
    assert_eq!(comments_and_anchors(e.document()), comments, "its comments come along on the same text");
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    let (again, _) = crate::docx::read_editable(&saved).expect("reads again");
    assert_eq!(lists_and_notes(&again), want, "as saved");
    assert_eq!(comments_and_anchors(&again), comments, "comments as saved");
}

/// Each comment's text and the body text it is anchored on, sorted.
fn comments_and_anchors(doc: &crate::docx::Document) -> Vec<(String, String)> {
    use crate::docx::review::RangeKind;
    let text = |blocks: &[Block]| -> String {
        blocks
            .iter()
            .filter_map(|b| if let Block::Para(p) = b { Some(super::super::text::UnitText::of(p).text) } else { None })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut anchored: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    let mut open: Vec<i64> = Vec::new();
    for b in doc.sections.iter().flat_map(|s| &s.blocks) {
        let Block::Para(p) = b else { continue };
        for i in &p.inlines {
            match i {
                Inline::Range(r) if r.kind == RangeKind::CommentStart => open.push(r.id),
                Inline::Range(r) if r.kind == RangeKind::CommentEnd => open.retain(|x| *x != r.id),
                Inline::Text(t, _) => open.iter().for_each(|id| anchored.entry(*id).or_default().push_str(t)),
                _ => {}
            }
        }
    }
    let mut out: Vec<(String, String)> =
        doc.comments.by_id.values().map(|c| (text(&c.body), anchored.get(&c.id.0).cloned().unwrap_or_default())).collect();
    out.sort();
    out
}

#[test]
fn a_sub_template_from_another_docx_takes_lists_of_its_own_and_no_notes() {
    use super::super::commands::Outcome;
    let (mut e, _) = expanded();
    let nums: Vec<i64> = e.document().resolver.as_ref().expect("styles").cascade().lists.lists.keys().map(|k| k.0).collect();
    assert!(matches!(create(&mut e, "Lists", Some(fixture_bytes("FX-CUSTOM-LISTS.docx"))), Outcome::Changed(_)));
    let res = e.document().resolver.as_ref().expect("styles");
    let part = e.document().glossary.as_ref().expect("a glossary").parts.iter().find(|b| b.name == "Lists").expect("made");
    let used: Vec<i64> =
        part.body.iter().filter_map(|b| if let Block::Para(p) = b { p.style.num.map(|n| n.list.0) } else { None }).filter(|n| *n != 0).collect();
    assert!(!used.is_empty());
    assert!(
        used.iter().all(|n| !nums.contains(n) && res.cascade().lists.lists.contains_key(&crate::document::paragraph::ListId(*n))),
        "{used:?} past {nums:?}"
    );
    assert!(matches!(create(&mut e, "Notes", Some(fixture_bytes("FX-A11Y-STRUCTURE.docx"))), Outcome::Refused(_)));
}

fn picture_bytes(doc: &crate::docx::Document) -> Vec<Vec<u8>> {
    let blip = |s: &Option<Box<crate::drawing::Shape>>| s.as_ref().and_then(|s| s.pic()).map(|p| p.blip.image);
    let mut out = Vec::new();
    for p in doc.sections.iter().flat_map(|s| &s.blocks).filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }) {
        for i in &p.inlines {
            let (image, drawn) = match i {
                Inline::Picture(pic, _) => (pic.image, blip(&pic.shape)),
                Inline::Float(f, _) => (f.image, blip(&f.shape)),
                _ => continue,
            };
            if let Some(drawn) = drawn {
                assert_eq!(drawn, image, "the shape draws the picture's own image");
            }
            out.extend(image.map(|k| doc.images.get(k).unwrap().to_vec()));
        }
    }
    out
}

#[test]
fn a_replaced_body_keeps_its_pictures_images() {
    let path = |n: &str| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(n);
    let mut e = open("FX-EXACT-ROW.docx");
    assert!(!e.document().images.is_empty(), "the template has an image of its own");
    let docx = std::fs::read(path("FX-OBJ.docx")).unwrap();
    let want = picture_bytes(&crate::docx::read_editable(&docx).unwrap().0);
    assert!(!want.is_empty());
    e.execute(Command::ReplaceBody(crate::api::command::ReplaceBody { from: crate::api::value::Bytes(docx) }), fonts());
    assert_eq!(picture_bytes(e.document()), want, "live");
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(picture_bytes(again.document()), want);
}

#[test]
fn a_replaced_body_keeps_the_bookmarks_it_shares_with_the_old_one() {
    let mark =
        |text: &str| format!(r#"<w:p><w:bookmarkStart w:id="0" w:name="Chapter"/><w:r><w:t>{text}</w:t></w:r><w:bookmarkEnd w:id="0"/></w:p>"#);
    let mut e = Editor::open(&super::blank_body(&mark("Old")), fonts()).expect("opens");
    e.complete(fonts());
    assert!(e.bookmark("Chapter").is_some(), "setup: the template's bookmark");
    let from = crate::api::value::Bytes(super::blank_body(&mark("New")));
    e.execute(Command::ReplaceBody(crate::api::command::ReplaceBody { from }), fonts());
    let found = e.bookmark("Chapter").expect("the new body's bookmark");
    assert_eq!(text_of(&e, found.start.para), "New");
    let again = Editor::open(&crate::docx::write::save(e.document(), e.docx()).expect("saves"), fonts()).expect("reopens");
    assert!(again.bookmark("Chapter").is_some(), "as saved");
}

/// A package of `body` with one list, `numId` 1, of format `fmt`.
fn numbered_package(body: &str, fmt: &str) -> Vec<u8> {
    use crate::layout::tests::package_bytes;
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const NS: &str = concat!(
        r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
        r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" "#,
        r#"xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup""#
    );
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#;
    let document = format!(
        r#"<?xml version="1.0"?><w:document {NS}><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let numbering = format!(
        r#"<?xml version="1.0"?><w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    package_bytes(&[
        ("[Content_Types].xml", ct),
        ("_rels/.rels", rels),
        ("word/_rels/document.xml.rels", doc_rels),
        ("word/document.xml", &document),
        ("word/numbering.xml", &numbering),
    ])
    .expect("a package")
}

#[test]
fn a_replaced_body_brings_the_lists_of_its_group_shapes_text() {
    use crate::layout::tests::shapes::{PT, anchor, sp};
    let item = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>item</w:t></w:r></w:p>"#;
    let boxed = sp(0, 0, 80, 80, "rect", "", "FFFFFF")
        .replace("<wps:bodyPr/>", &format!("<wps:txbx><w:txbxContent>{item}</w:txbxContent></wps:txbx><wps:bodyPr/>"));
    let group = format!(
        r#"<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{w}"/><a:chOff x="0" y="0"/><a:chExt cx="{w}" cy="{w}"/></a:xfrm></wpg:grpSpPr>{boxed}</wpg:wgp>"#,
        w = 100 * PT,
    );
    let source = numbered_package(&format!("<w:p>{}</w:p>", anchor(72, 300, 100, 100, 1, false, 1, &group)), "upperRoman");
    let mut e = Editor::open(&numbered_package(item, "decimal"), fonts()).expect("opens");
    e.complete(fonts());
    let formats = |doc: &crate::docx::Document| -> Vec<String> {
        let res = doc.resolver.as_ref().expect("styles");
        let mut out = Vec::new();
        for p in doc.sections.iter().flat_map(|s| &s.blocks).filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }) {
            for i in &p.inlines {
                let Inline::Float(f, _) = i else { continue };
                for q in f.parts.iter().filter_map(|g| g.text_box.as_ref()).flat_map(|t| &t.blocks) {
                    let Block::Para(q) = q else { continue };
                    out.extend(q.style.num.and_then(|ListRef { list, level }| {
                        res.cascade().level(list, level).map(|l| crate::docx::list_xml::format_name(l.format.as_ref()).to_string())
                    }));
                }
            }
        }
        out
    };
    assert_eq!(formats(&crate::docx::read_editable(&source).expect("reads").0), ["upperRoman"], "setup: the source's list");
    e.execute(Command::ReplaceBody(crate::api::command::ReplaceBody { from: crate::api::value::Bytes(source) }), fonts());
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    assert_eq!(formats(&crate::docx::read_editable(&saved).expect("reads again").0), ["upperRoman"], "its own list, as saved");
}

#[test]
fn a_range_on_a_shown_body_keeps_every_range_from_formatting() {
    let (mut e, reference) = expanded();
    click_line(&mut e, reference, 1);
    let (a, b) = e.selection().ordered(e.index()).expect("the reference field");
    let after = body_block(&e, 0, 2);
    let story = a.story;
    let free = (super::super::position::Position::text(story, after, Unit(0)), super::super::position::Position::text(story, after, Unit(2)));
    e.select_ranges(&[(a, b), free]);
    assert!(e.is_discontiguous());
    let before = e.document().sections[0].blocks.clone();
    let free_text = text_of(&e, after);
    assert!(free_text.chars().count() > 2, "setup: {free_text:?}");
    let out = e.execute(Command::SetBold(true), fonts());
    assert!(!matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    assert_eq!(e.document().sections[0].blocks, before, "neither range formatted");
    let out = e.execute(Command::InsertText("x".into()), fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "typing replaces the free range alone: {out:?}");
    assert_eq!(e.document().sections[0].blocks[1], before[1], "the reference is kept");
    let want: String = std::iter::once('x').chain(free_text.chars().skip(2)).collect();
    assert_eq!(text_of(&e, after), want, "the free range's two units are replaced by the typed text, the rest kept");
}

#[test]
fn a_shown_body_is_checked_when_a_cell_block_is_the_selection() {
    let (mut e, reference) = expanded();
    click_line(&mut e, reference, 1);
    let (a, b) = e.selection().ordered(e.index()).expect("the reference field");
    let table = e.document().sections[0]
        .blocks
        .iter()
        .find_map(|b| if let Block::Table(t) = b { Some(crate::layout::TableId(t.id)) } else { None })
        .expect("a table");
    let cell = crate::editing::selection::CellRef { row: 0, col: 0 };
    let cells = crate::editing::selection::Selection::Cells { table, anchor: cell, focus: cell, shape: crate::editing::selection::CellShape::Rect };
    e.select_pieces(&[(a, b)], Vec::new(), Some(cells));
    assert!(matches!(e.selection(), crate::editing::selection::Selection::Cells { .. }) && e.is_discontiguous());
    let before = e.document().sections[0].blocks.clone();
    let out = e.execute(Command::SetBold(true), fonts());
    assert!(!matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    assert_eq!(e.document().sections[0].blocks, before, "the reference's range is checked whichever range is the selection");
}

#[test]
fn a_shown_body_in_a_tbrl_section_is_hit_and_outlined_where_it_is_drawn() {
    let (mut e, reference) = expanded();
    e.execute(Command::SetSectionTextDirection(crate::api::command::SectionValue::here(crate::api::command::TextFlow::TbRl)), fonts());
    e.complete(fonts());
    let field = e.expansions.as_ref().and_then(|x| x.paras.get(&reference)).map(|x| x.field).expect("shown");
    let (pg, g) = e.page_geometries().into_iter().find(|(_, g)| g.lines.iter().any(|l| l.shows(reference))).expect("a page");
    let l = g.lines.iter().find(|l| l.shows(reference)).expect("a line");
    assert_eq!(l.dir(), super::super::geometry::LineDir::Down);
    let band = l.band(l.x0(), l.x1());
    let rects = e.expansion_rects(field, pg..pg + 1);
    let &(_, r) = rects.first().expect("outlined");
    assert!(r[0] >= band[0] - 0.5 && r[2] <= band[2] + 0.5 && r[3] - r[1] > r[2] - r[0], "down the line's band: {r:?} in {band:?}");
    let (x, y) = e.stack().to_doc(pg, (r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0).expect("on the stack");
    assert_eq!(e.expansion_at(x, y).map(|h| h.0), Some(field), "hit where it is drawn");
}

#[test]
fn the_current_heading_is_the_sub_template_s_while_it_is_edited() {
    let (mut e, reference) = expanded();
    click_line(&mut e, reference, 1);
    click_line(&mut e, reference, 2);
    assert_eq!(e.editing_sub_template(), Some(0));
    let first = body_para(&e, 0, 0);
    e.select_range(first, Unit(0), Unit(0), fonts());
    assert_eq!(e.outline_level(first), Some(1), "Violation's heading");
    let id = crate::api::value::HeadingId(first.0.to_string());
    assert!(crate::services::headings(&mut e).iter().any(|h| h.id == id), "listed");
    assert_eq!(crate::services::current_heading(&e), Some(id));
}
