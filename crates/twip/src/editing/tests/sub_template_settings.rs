//! Document settings and page setup while a sub-template is shown or edited

use super::super::Editor;
use super::super::commands::Command;
use super::super::state::StateFlags;
use super::{fonts, open};
use crate::api::command::{CommandId as C, SubTemplateName};
use crate::api::value::Points;
use crate::docx::Block;
use crate::layout::ParaId;

fn with_rules() -> Editor {
    let mut e = open("FX-SUBTEMPLATES.docx");
    e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    e.complete(fonts());
    e
}

fn violation() -> Command {
    Command::EditSubTemplate(SubTemplateName { name: "Violation".into(), category: None })
}

fn line_ends(e: &mut Editor) -> Vec<f64> {
    let (_, g) = e.page_geometries().into_iter().next().expect("a page");
    g.lines.iter().filter_map(|l| l.stops.last().map(|s| s.x)).collect()
}

#[test]
fn a_default_tab_stop_set_in_a_sub_template_relays_the_document_on_close() {
    let mut e = open("FX-SUBTEMPLATES.docx");
    e.complete(fonts());
    e.execute(Command::InsertTab, fonts());
    e.complete(fonts());
    let before = line_ends(&mut e);
    e.execute(violation(), fonts());
    e.execute(Command::SetDefaultTabStop(Points(144.0)), fonts());
    e.execute(Command::CloseSubTemplate, fonts());
    e.complete(fonts());
    assert_eq!(e.editing_sub_template(), None);
    let live = line_ends(&mut e);
    assert_ne!(live, before, "the tab stop moves the tab");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let mut fresh = Editor::open(&saved, fonts()).expect("reopens");
    fresh.complete(fonts());
    assert_eq!(live, line_ends(&mut fresh));
}

#[test]
fn a_default_tab_stop_change_relays_the_shown_expansions() {
    let mut e = with_rules();
    let before: Vec<_> = e.expansions.as_ref().expect("expansions").paras.values().map(|x| x.content.clone()).collect();
    assert!(!before.is_empty());
    e.execute(Command::SetDefaultTabStop(Points(144.0)), fonts());
    e.complete(fonts());
    let after: Vec<_> = e.expansions.as_ref().expect("expansions").paras.values().map(|x| x.content.clone()).collect();
    assert!(before.iter().zip(&after).all(|(a, b)| !std::rc::Rc::ptr_eq(a, b)), "the expansions were laid out again");
}

#[test]
fn new_hyphenation_patterns_relay_the_shown_expansions() {
    let mut e = with_rules();
    let before: Vec<_> = e.expansions.as_ref().expect("expansions").paras.values().map(|x| x.content.clone()).collect();
    e.set_hyphenation(Some(crate::layout::hyphenate::Hyphenation::default()));
    e.complete(fonts());
    let after: Vec<_> = e.expansions.as_ref().expect("expansions").paras.values().map(|x| x.content.clone()).collect();
    assert!(before.iter().zip(&after).all(|(a, b)| !std::rc::Rc::ptr_eq(a, b)), "the expansions were laid out again");
}

#[test]
fn the_sub_template_surface_follows_the_default_tab_stop() {
    let mut e = with_rules();
    e.execute(violation(), fonts());
    e.complete(fonts());
    assert_eq!(e.editing_sub_template(), Some(0));
    e.execute(Command::InsertTab, fonts());
    e.complete(fonts());
    let before = line_ends(&mut e);
    e.execute(Command::SetDefaultTabStop(Points(144.0)), fonts());
    e.complete(fonts());
    assert_ne!(line_ends(&mut e), before, "the tab now reaches 144 pt");
}

fn body_para(e: &Editor, s: usize, i: usize) -> ParaId {
    match &e.document().sections[s].blocks[i] {
        Block::Para(p) => ParaId(p.id),
        Block::Table(_) => panic!("a table"),
    }
}

/// Double-clicks the expansion in section 1's first paragraph, a landscape section.
fn entered_from_the_landscape_section() -> Editor {
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    let mut e = with_rules();
    let para = body_para(&e, 1, 0);
    let (pg, g) = e.page_geometries().into_iter().find(|(_, g)| g.lines.iter().any(|l| l.shows(para))).expect("a page");
    let l = g.lines.iter().find(|l| l.shows(para)).expect("a line");
    let (x, y) = e.stack().to_doc(pg, l.stops[0].x + 40.0, (l.top + l.bottom) / 2.0).expect("on the stack");
    for c in 1..=2 {
        e.pointer_input(PointerKind::Down, x, y, c, Mods::default(), fonts());
        e.pointer_input(PointerKind::Up, x, y, c, Mods::default(), fonts());
    }
    assert_eq!(e.editing_sub_template(), Some(0));
    e.complete(fonts());
    e
}

#[test]
fn page_state_in_a_sub_template_is_the_section_it_was_entered_from() {
    let mut e = entered_from_the_landscape_section();
    assert_eq!(e.snapshot().pages()[0].w, 792.0, "the shown pages are landscape");
    let f = e.format_state().expect("a state");
    assert_eq!(f.section, 1);
    assert_eq!(e.page_formatting(f.section).size.width.0, 792.0);
}

#[test]
fn page_setup_in_a_sub_template_changes_the_section_it_shows() {
    let mut e = entered_from_the_landscape_section();
    let width = |e: &mut Editor| e.page_geometries().into_iter().next().map(|(_, g)| g.lines[0].stops.last().map_or(0.0, |s| s.x));
    let before = width(&mut e);
    let (left0, left1) = (e.document().sections[0].left, e.document().sections[1].left);
    let margins = crate::api::command::Margins { left: Some(Points(left1 + 72.0)), ..Default::default() };
    e.execute(Command::SetMargins(margins), fonts());
    e.complete(fonts());
    assert_eq!((e.document().sections[0].left, e.document().sections[1].left), (left0, left1 + 72.0));
    assert_ne!(width(&mut e), before, "the surface shows the new margin");
}

#[test]
fn section_commands_are_disabled_in_a_sub_template() {
    let mut e = entered_from_the_landscape_section();
    let f = e.format_state();
    let states = e.command_states(f.as_ref(), StateFlags { read_only: false });
    let on: Vec<_> = [
        C::SetMargins,
        C::SetPageNumbering,
        C::SetPageVerticalAlignment,
        C::SetLineNumbering,
        C::SetSectionBreakType,
        C::SetSectionTextDirection,
        C::SetHeaderFooterLinked,
        C::RemoveHeaderFooter,
    ]
    .into_iter()
    .filter(|id| states[id].enabled)
    .collect();
    assert!(on.is_empty(), "{on:?}");
    assert!(states[&C::SetDefaultTabStop].enabled, "document settings apply, and the surface follows them");
}
