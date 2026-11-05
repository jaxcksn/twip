//! Sub-templates

use super::super::Editor;
use super::super::commands::Command;
use super::super::elements;
use super::super::position::StoryId;
use super::{fonts, open};
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
    e.execute(Command::Undo, fonts());
    assert_eq!(text_of(&e, p), "Violation notice");
    e.execute(Command::Redo, fonts());
    assert_eq!(text_of(&e, p), "New Violation notice");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let again = crate::docx::read_editable(&saved).expect("reads");
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
    e.execute(Command::Undo, fonts());
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
    let cycle = shown.paras.values().find(|x| x.part == Some(3)).expect("Cycle-A");
    assert!(cycle.size.1 > 20.0);
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
    e.execute(Command::Undo, fonts());
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
    e.execute(Command::Undo, fonts());
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
    e.execute(Command::Undo, fonts());
    assert!(!part_names(&e).contains(&"Imported again".into()));
    let rename =
        crate::api::command::RenameSubTemplate { name: "Violation".into(), to: "Citation".into(), category: None, update_references: Some(true) };
    assert!(matches!(e.execute(Command::RenameSubTemplate(rename), fonts()), Outcome::Changed(_)));
    assert!(part_names(&e).contains(&"Citation".into()));
    assert!(codes(&e).iter().any(|c| c.contains("SUBTEMPLATE Citation")));
    assert!(!codes(&e).iter().any(|c| c.contains("SUBTEMPLATE Violation")));
    let resolved = e.references().by_field.values().filter(|r| r.name == "Citation" && r.resolution == R::Resolved).count();
    assert!(resolved >= 2);
    e.execute(Command::Undo, fonts());
    assert!(part_names(&e).contains(&"Violation".into()) && codes(&e).iter().any(|c| c.contains("SUBTEMPLATE Violation")));
    let remove = |n: &str| Command::RemoveSubTemplate(crate::api::command::SubTemplateName { name: n.into(), category: None });
    e.execute(remove("Cycle-B"), fonts());
    assert!(!part_names(&e).contains(&"Cycle-B".into()));
    assert!(e.references().by_field.values().any(|r| r.name == "Cycle-B" && r.resolution == R::Missing));
    e.execute(Command::Undo, fonts());
    assert!(e.references().by_field.values().any(|r| r.name == "Cycle-B" && r.resolution == R::Resolved));
    e.execute(Command::EditSubTemplate(crate::api::command::SubTemplateName { name: "Appeals notice".into(), category: None }), fonts());
    e.execute(remove("Appeals notice"), fonts());
    assert_eq!(e.editing_sub_template(), None);
    assert!(!part_names(&e).contains(&"Appeals notice".into()));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let again = crate::docx::read_editable(&saved).expect("reads");
    let names: Vec<&str> = again.glossary.as_ref().unwrap().parts.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Violation", "FeeRow", "Cycle-A", "Cycle-B", "Greeting", "Payment", "Imported"]);
}

#[test]
fn the_first_sub_template_makes_the_glossary_part() {
    let mut e = open("FX-EMPTY.docx");
    e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    assert!(matches!(create(&mut e, "Notice", None), super::super::commands::Outcome::Changed(_)));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let z = zip::ZipArchive::new(std::io::Cursor::new(saved.clone())).expect("a package");
    assert!(z.file_names().any(|n| n.is_ok_and(|n| n == "word/glossary/document.xml")));
    let again = crate::docx::read_editable(&saved).expect("reads");
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
