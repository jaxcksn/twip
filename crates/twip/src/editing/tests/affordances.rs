//! Placeholder affordances: field marks, their states and what they leave alone

use super::super::Editor;
use super::super::Mode;
use super::super::affordances::{AffordanceView, FieldState, Segment, Target};
use super::super::commands::Command;
use super::{fonts, open};
use crate::api::command::DeleteOptions;
use crate::api::value::Direction;

const FIXTURES: [&str; 6] = [
    "FX-PLACEHOLDER-UI.docx",
    "FX-MERGE-FIELDS.docx",
    "FX-MERGE-CONTROLS.docx",
    "FX-MERGE-REGIONS.docx",
    "FX-SUBTEMPLATES.docx",
    "FX-PLACEHOLDER.docx",
];

fn opened(name: &str, mode: Mode, on: bool) -> Editor {
    let mut e = open(name);
    e.view_settings.placeholder_ui = on;
    e.set_mode(mode);
    e.complete(fonts());
    e
}

fn design(name: &str) -> Editor {
    opened(name, Mode::Design, true)
}

fn pages(e: &mut Editor) -> usize {
    e.page_geometries().len()
}

fn views(e: &mut Editor) -> Vec<AffordanceView> {
    let n = pages(e);
    e.view(0..n).affordances
}

/// Each field mark's key and its segments, by page.
fn marks(e: &mut Editor) -> Vec<(String, usize, Vec<Segment>, FieldState)> {
    let all = views(e);
    all.into_iter()
        .filter_map(|v| match v {
            AffordanceView::Field { page, target, segments, state, .. } => {
                let key = e.describe(target).and_then(|d| d.key).unwrap_or_default();
                Some((key, page, segments, state))
            }
            _ => None,
        })
        .collect()
}

fn target_with(e: &mut Editor, key: &str) -> Target {
    let all = views(e);
    all.into_iter()
        .filter_map(|v| match v {
            AffordanceView::Field { target, .. } => Some(target),
            _ => None,
        })
        .find(|t| e.describe(*t).and_then(|d| d.key).as_deref() == Some(key))
        .unwrap_or_else(|| panic!("a mark for {key}"))
}

fn select(e: &mut Editor, t: Target) {
    let id = crate::api::value::ElementId(super::super::elements::id_str(t.element()));
    e.select_element(&id, false, fonts());
}

#[test]
fn nothing_is_drawn_while_off_and_only_blocks_and_bodies_outside_design_mode() {
    for name in FIXTURES {
        for mode in [Mode::Design, Mode::Editing] {
            let mut e = opened(name, mode, false);
            assert!(views(&mut e).is_empty(), "{name} in {mode:?}, off");
        }
        let mut e = opened(name, Mode::Editing, true);
        let design_only = views(&mut e).into_iter().filter(|v| !matches!(v, AffordanceView::Block(_) | AffordanceView::BodyMark { .. })).count();
        assert_eq!(design_only, 0, "{name} in editing mode");
    }
}

#[test]
fn a_wrapped_field_has_a_segment_per_line_rounded_only_at_its_ends() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let all = marks(&mut e);
    let (_, _, segs, state) = all.iter().find(|m| m.0 == "Applicant.Organisation_Name_As_Registered").expect("the wrapping field");
    assert_eq!(*state, FieldState::Rest);
    assert_eq!(segs.len(), 2, "{segs:?}");
    assert!(segs[0].round_left && !segs[0].round_right, "{:?}", segs[0]);
    assert!(!segs[1].round_left && segs[1].round_right, "{:?}", segs[1]);
    assert!(segs[1].rect[1] > segs[0].rect[3] - 0.01, "the second segment is on the next line");
}

#[test]
fn adjacent_fields_are_drawn_apart() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let all = marks(&mut e);
    let first = &all.iter().find(|m| m.0 == "Applicant.First_Name").expect("the first name").2[0];
    let last = &all.iter().find(|m| m.0 == "Applicant.Last_Name").expect("the last name").2[0];
    assert!((first.rect[2] - last.rect[0]).abs() < 0.01, "they touch: {first:?} {last:?}");
    assert!(first.gap_right && last.gap_left && !first.gap_left && !last.gap_right);
}

#[test]
fn merge_fields_and_placeholder_controls_are_marked_and_evaluated_fields_are_not() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let keys: Vec<String> = marks(&mut e).into_iter().map(|m| m.0).collect();
    for key in [
        "Case.Number",
        "Applicant.Email",
        "Applicant.Website",
        "Parcel.Id",
        "Permit.Number",
        "Applicant/Phone",
        "Permit/Remarks",
        "Permit/Date",
        "Permit/Decision",
        "Permit/Officer",
    ] {
        assert!(keys.iter().any(|k| k == key), "{key} in {keys:?}");
    }
    let mut f = design("FX-MERGE-FIELDS.docx");
    let all = views(&mut f);
    let codes: Vec<String> = all
        .into_iter()
        .filter_map(|v| match v {
            AffordanceView::Field { target, .. } => f.describe(target).and_then(|d| d.code),
            _ => None,
        })
        .collect();
    assert!(codes.iter().any(|c| c.starts_with("MERGEFIELD Statement.Cost_Amount")), "{codes:?}");
    assert!(!codes.iter().any(|c| c == "PAGE" || c == "NUMPAGES" || c.starts_with("IF ")), "{codes:?}");
}

#[test]
fn a_field_shown_as_its_code_has_no_mark() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let before = marks(&mut e).iter().filter(|m| m.0.contains('.')).count();
    e.show_field_codes(true, None);
    e.complete(fonts());
    let after = marks(&mut e).iter().filter(|m| m.0.contains('.')).count();
    assert!(before > 0 && after == 0, "{before} marks, then {after}");
}

#[test]
fn field_marks_never_break_a_line_differently() {
    // Each line's paragraph and units: where lines break. A repeating section's band moves lines down, never apart.
    for name in FIXTURES {
        let lines = |on: bool| {
            let mut e = opened(name, Mode::Design, on);
            // Paragraph ids differ between two opens: their place in the document doesn't.
            let order = super::texts_ids(&e);
            let at = |p: Option<crate::layout::ParaId>| p.and_then(|p| order.iter().position(|q| *q == p)).map(|k| k as u64);
            let mut all: Vec<_> = e
                .page_geometries()
                .into_iter()
                .flat_map(|(_, g)| {
                    g.lines
                        .iter()
                        .filter(|l| !l.repeat)
                        .map(|l| (at(l.para()), l.stops.first().map(|s| s.unit), l.stops.last().map(|s| s.unit)))
                        .collect::<Vec<_>>()
                })
                .collect();
            all.sort();
            all
        };
        let (off, on) = (lines(false), lines(true));
        type Line = (Option<u64>, Option<crate::layout::Unit>, Option<crate::layout::Unit>);
        let only = |a: &[Line], b: &[Line]| a.iter().filter(|l| !b.contains(l)).copied().collect::<Vec<Line>>();
        assert!(off == on, "{name}: only off {:?}, only on {:?}", only(&off, &on), only(&on, &off));
    }
}

#[test]
fn a_clicked_field_is_selected_whole_and_typing_replaces_it_in_one_step() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let t = target_with(&mut e, "Applicant.Email");
    select(&mut e, t);
    assert_eq!(e.selected_target(), Some(t));
    let states: Vec<FieldState> = marks(&mut e).into_iter().filter(|m| m.0 == "Applicant.Email").map(|m| m.3).collect();
    assert_eq!(states, vec![FieldState::Selected]);
    let depth = e.history().undo_depth();
    e.execute(Command::InsertText("x".into()), fonts());
    assert!(!marks(&mut e).iter().any(|m| m.0 == "Applicant.Email"), "typing replaced the field");
    assert_eq!(e.history().undo_depth(), depth + 1);
    e.execute(Command::Undo, fonts());
    let t = target_with(&mut e, "Applicant.Email");
    select(&mut e, t);
    e.execute(Command::Delete(DeleteOptions { direction: Direction::Backward, unit: None }), fonts());
    assert!(!marks(&mut e).iter().any(|m| m.0 == "Applicant.Email"), "Backspace deleted the field");
}

#[test]
fn a_locked_control_selects_but_refuses_typing() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let t = target_with(&mut e, "Permit/Officer");
    select(&mut e, t);
    assert_eq!(e.selected_target(), Some(t));
    let depth = e.history().undo_depth();
    e.execute(Command::InsertText("x".into()), fonts());
    assert_eq!(e.history().undo_depth(), depth, "the edit was refused");
}

/// Each repeating section's views by its title: (page, bar, tab, rows).
fn regions(e: &mut Editor) -> Vec<(String, usize, [f64; 4], Option<super::super::view::Tab>, Vec<[f64; 4]>)> {
    let all = views(e);
    all.into_iter()
        .filter_map(|v| match v {
            AffordanceView::Region { page, target, bar, tab, rows, .. } => {
                let label = e.describe(target).map(|d| d.label).unwrap_or_default();
                Some((label, page, bar, tab, rows))
            }
            _ => None,
        })
        .collect()
}

fn region<'a>(
    all: &'a [(String, usize, [f64; 4], Option<super::super::view::Tab>, Vec<[f64; 4]>)],
    title: &str,
) -> Vec<&'a (String, usize, [f64; 4], Option<super::super::view::Tab>, Vec<[f64; 4]>)> {
    let found: Vec<_> = all.iter().filter(|r| r.0 == title).collect();
    assert!(!found.is_empty(), "{title} in {:?}", all.iter().map(|r| &r.0).collect::<Vec<_>>());
    found
}

#[test]
fn nested_sections_have_bars_eight_px_apart_and_tabs_in_their_bands() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let all = regions(&mut e);
    let u = 0.75;
    let (outer, inner) = (region(&all, "Sections")[0], region(&all, "Conditions")[0]);
    assert!((outer.2[0] - inner.2[0] - 8.0 * u).abs() < 0.01, "the inner bar is 8 px further out: {:?} {:?}", outer.2, inner.2);
    assert!((outer.2[2] - outer.2[0] - 2.0 * u).abs() < 0.01, "2 px wide");
    let tab = outer.3.as_ref().expect("the outer tab");
    assert!(tab.text.starts_with("each Sections") && tab.text.ends_with("2 levels"), "{}", tab.text);
    assert!((tab.rect[3] - outer.2[1]).abs() < 0.01, "the tab sits on the section's top: {:?} {:?}", tab.rect, outer.2);
    assert!(outer.4.is_empty(), "paragraph sections aren't tinted");
}

#[test]
fn a_section_from_a_later_row_has_its_tab_beside_the_row_and_rows_are_tinted() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let all = regions(&mut e);
    let owners = region(&all, "Owners")[0];
    let tab = owners.3.as_ref().expect("its tab");
    assert!(tab.rect[2] <= owners.2[0] + 0.01 && (tab.rect[1] - owners.2[1]).abs() < 0.01, "beside the row: {:?} {:?}", tab.rect, owners.2);
    assert_eq!(owners.4.len(), 1, "its row is tinted");
    let whole = region(&all, "Inspections")[0];
    assert!(whole.4.is_empty(), "a section around a whole table tints no rows");
}

#[test]
fn a_section_across_a_page_break_has_a_bar_on_each_page_and_none_beside_the_repeated_header() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let all = regions(&mut e);
    let lines = region(&all, "Lines");
    assert!(lines.len() >= 2, "a view on each page: {:?}", lines.iter().map(|l| l.1).collect::<Vec<_>>());
    assert!(lines[0].3.is_some() && lines[1].3.is_none(), "the tab on the first page only");
    let next = lines[1];
    let geo = e.page_geometries().into_iter().find(|(p, _)| *p == next.1).map(|(_, g)| g).expect("the page");
    let header = geo.rows.iter().find(|r| r.repeat).expect("the repeated header row");
    assert!(next.2[1] >= header.rect[3] - 0.01, "the bar starts under the header: {:?} {:?}", next.2, header.rect);
}

#[test]
fn a_bands_room_is_kept_above_a_section_and_nowhere_in_the_output() {
    let first_line = |on: bool| {
        let mut e = opened("FX-PLACEHOLDER-UI.docx", Mode::Design, on);
        let order = super::texts_ids(&e);
        let section = section_para(&e);
        let geo = e.page_geometries();
        let tops: Vec<f64> = geo.iter().flat_map(|(_, g)| g.lines.iter().filter(|l| l.para() == Some(section)).map(|l| l.top)).collect();
        let before: Vec<f64> = geo
            .iter()
            .flat_map(|(_, g)| {
                g.lines
                    .iter()
                    .filter(|l| {
                        l.para().is_some_and(|p| order.iter().position(|q| *q == p) == order.iter().position(|q| *q == section).map(|k| k - 1))
                    })
                    .map(|l| l.bottom)
            })
            .collect();
        tops[0] - before.last().copied().expect("the line before")
    };
    let (off, on) = (first_line(false), first_line(true));
    assert!(on >= 12.0 - 0.01 && on > off, "the gap above the section is at least the band: {off} then {on}");
}

#[test]
fn incremental_layout_with_bands_equals_a_full_one() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let check = |e: &mut Editor, step: &str| {
        e.complete(fonts());
        let shown = e.expansions.clone().unwrap_or_default();
        assert!(!shown.bands.is_empty() && !shown.tables.is_empty(), "bands are kept");
        let ours = crate::layout::dump::page_text(&e.snapshot(), e.document());
        let fresh = crate::layout::dump::page_text(&crate::layout::document::layout_expanded(e.document(), fonts(), &shown), e.document());
        assert!(ours == fresh, "{step}: incremental layout differs from a full one");
    };
    check(&mut e, "on");
    let section = section_para(&e);
    e.select_range(section, crate::layout::Unit(0), crate::layout::Unit(0), fonts());
    for _ in 0..30 {
        e.execute(Command::InsertText("long words ".into()), fonts());
    }
    check(&mut e, "typed");
}

#[test]
fn a_press_on_a_tab_selects_its_rows() {
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let all = regions(&mut e);
    let owners = region(&all, "Owners")[0];
    let tab = owners.3.clone().expect("its tab");
    let (x, y) = e.stack().to_doc(owners.1, (tab.rect[0] + tab.rect[2]) / 2.0, (tab.rect[1] + tab.rect[3]) / 2.0).expect("on the page");
    for kind in [PointerKind::Down, PointerKind::Up] {
        e.pointer_input(kind, x, y, 1, Mods::default(), fonts());
    }
    assert!(matches!(e.selection(), super::super::selection::Selection::Cells { .. }), "{:?}", e.selection());
    assert!(matches!(e.selected_target(), Some(Target::RepeatingSection(_))));
}

/// The outer repeating section's first paragraph.
fn section_para(e: &Editor) -> crate::layout::ParaId {
    let (ids, texts) = (super::texts_ids(e), super::texts(e));
    ids[texts.iter().position(|t| t.starts_with("Section ")).expect("the section's paragraph")]
}

/// The sub-template fixture with its rules, in `mode` with the affordances on.
fn sub_templates(mode: Mode) -> Editor {
    let mut e = open("FX-SUBTEMPLATES.docx");
    e.view_settings.placeholder_ui = true;
    e.execute(super::super::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    e.set_mode(mode);
    e.complete(fonts());
    e
}

fn collapse_all(e: &mut Editor) {
    e.execute(Command::SetSubTemplateView(crate::api::command::SubTemplateView { ids: None, expanded: false }), fonts());
    e.complete(fonts());
}

/// The cards drawn: (name, line, resolution, rect, expand).
fn cards(e: &mut Editor) -> Vec<(String, String, super::super::view::CardState, [f64; 4], Option<[f64; 4]>, usize)> {
    views(e)
        .into_iter()
        .filter_map(|v| match v {
            AffordanceView::Card { page, name, line, resolution, rect, expand, .. } => Some((name, line, resolution, rect, expand, page)),
            _ => None,
        })
        .collect()
}

fn body_marks(e: &mut Editor) -> Vec<(super::super::view::Tab, Vec<(super::super::view::Part, [f64; 4])>, usize)> {
    views(e)
        .into_iter()
        .filter_map(|v| match v {
            AffordanceView::BodyMark { page, tab, parts, .. } => Some((tab, parts, page)),
            _ => None,
        })
        .collect()
}

fn press(e: &mut Editor, page: usize, r: [f64; 4]) {
    use super::super::input::PointerKind;
    use super::super::keymap::Mods;
    let (x, y) = e.stack().to_doc(page, (r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0).expect("on the page");
    for kind in [PointerKind::Down, PointerKind::Up] {
        e.pointer_input(kind, x, y, 1, Mods::default(), fonts());
    }
    e.complete(fonts());
}

#[test]
fn collapsed_references_are_cards_by_resolution() {
    use super::super::view::CardState;
    let mut e = sub_templates(Mode::Design);
    collapse_all(&mut e);
    let all = cards(&mut e);
    let violation = all.iter().find(|c| c.0 == "Violation").expect("its card");
    assert_eq!(violation.2, CardState::Resolved);
    assert!((violation.3[3] - violation.3[1] - super::super::affordances::CARD_HEIGHT).abs() < 0.01, "{:?}", violation.3);
    assert!(violation.1.starts_with("sub-template") && violation.1.ends_with("fields"), "{}", violation.1);
    assert!(violation.4.is_some(), "Expand");
    let witness = all.iter().find(|c| c.0 == "Witness").expect("the missing one's card");
    assert_eq!((witness.2, witness.1.as_str(), witness.4), (CardState::Missing, "not found", None));
    let near = all.iter().find(|c| c.0 == "violation").expect("the near match's card");
    assert_eq!((near.2, near.1.as_str()), (CardState::NearMatch, "did you mean Violation?"));
    // A reference sharing its paragraph is a field mark, never a card.
    let marked: Vec<Target> =
        views(&mut e).into_iter().filter_map(|v| if let AffordanceView::Field { target, .. } = v { Some(target) } else { None }).collect();
    assert!(
        marked.iter().any(|t| e.describe(*t).and_then(|d| d.code).is_some_and(|c| c.contains("SUBTEMPLATE Violation"))),
        "the reference in a sentence"
    );
}

#[test]
fn expand_shows_a_card_without_an_undo_step_and_the_body_keeps_its_tab_in_every_mode() {
    let mut e = sub_templates(Mode::Design);
    collapse_all(&mut e);
    let depth = e.history().undo_depth();
    let violations = |e: &mut Editor| cards(e).into_iter().filter(|c| c.0 == "Violation").count();
    let before = violations(&mut e);
    let card = cards(&mut e).into_iter().find(|c| c.0 == "Violation").expect("its card");
    press(&mut e, card.5, card.4.expect("Expand"));
    assert_eq!(violations(&mut e), before - 1, "no longer a card");
    assert_eq!(e.history().undo_depth(), depth);
    assert_eq!(e.editing_sub_template(), None);
    let marks = body_marks(&mut e);
    let (tab, parts, _) = marks.iter().find(|m| m.0.text.starts_with("Violation")).expect("the shown body's tab");
    assert!(tab.text.contains("read-only") && tab.text.contains("Open \u{2197}"), "{}", tab.text);
    assert!(parts.iter().any(|p| p.0 == super::super::view::Part::Open));
    e.set_mode(Mode::Editing);
    e.complete(fonts());
    assert!(body_marks(&mut e).iter().any(|m| m.0.text.starts_with("Violation")), "in editing mode too");
    assert!(cards(&mut e).is_empty(), "cards are design mode's");
}

#[test]
fn open_enters_the_sub_template_and_a_newer_version_asks_the_application() {
    use super::super::input::Notice;
    use super::super::view::Part;
    let mut e = sub_templates(Mode::Design);
    e.execute(
        Command::SetSubTemplateVersions(crate::api::command::SubTemplateVersions {
            versions: vec![crate::api::command::SubTemplateVersion {
                name: "Violation".into(),
                category: None,
                version: "v4".into(),
                newer: Some("v5 available".into()),
            }],
        }),
        fonts(),
    );
    e.complete(fonts());
    let marks = body_marks(&mut e);
    let (tab, parts, page) = marks.into_iter().find(|m| m.0.text.starts_with("Violation")).expect("the tab");
    assert!(tab.text.contains("v4") && tab.text.ends_with("v5 available"), "{}", tab.text);
    e.take_notices();
    let update = parts.iter().find(|p| p.0 == Part::Update).expect("the newer version").1;
    press(&mut e, page, update);
    assert!(
        e.take_notices()
            .iter()
            .any(|n| matches!(n, Notice::SubTemplateUpdate { name, category } if name == "Violation" && category == "Sub-templates"))
    );
    let open = parts.iter().find(|p| p.0 == Part::Open).expect("Open").1;
    press(&mut e, page, open);
    assert!(e.editing_sub_template().is_some(), "editing the sub-template");
}

#[test]
fn cards_and_tabs_are_off_with_the_affordances() {
    let mut e = sub_templates(Mode::Design);
    e.view_settings.placeholder_ui = false;
    collapse_all(&mut e);
    assert!(views(&mut e).is_empty());
    assert!(e.expansions.as_ref().is_none_or(|x| x.paras.values().all(|p| !p.content.is_empty() && p.band == 0.0)));
}

/// The placeholder fixture in editing mode with the affordances on and a look for signatures: a colour, label, icon,
/// hint and two buttons.
fn signing() -> Editor {
    use crate::api::command::{PlaceholderAction, PlaceholderLook, PlaceholderLookRule, PlaceholderLooks, PlaceholderMatch};
    let mut e = opened("FX-PLACEHOLDER-UI.docx", Mode::Editing, true);
    let rule = PlaceholderLookRule {
        matches: PlaceholderMatch::Prefix { prefix: "Signature".into() },
        look: PlaceholderLook::Drawn {
            fill: Some(crate::api::value::Color(0x2E7D32)),
            label: Some("Signature".into()),
            icon_svg: Some(r#"<svg viewBox="0 0 10 10"><path d="M0 10 L10 0"/></svg>"#.into()),
        },
        hint: Some("Click to sign".into()),
        actions: Some(vec![
            PlaceholderAction { id: "sign".into(), label: "Sign".into(), primary: Some(true) },
            PlaceholderAction { id: "upload".into(), label: "Upload image".into(), primary: None },
        ]),
    };
    e.execute(Command::SetPlaceholderLooks(PlaceholderLooks { looks: vec![rule], merge: None }), fonts());
    e.complete(fonts());
    e
}

fn blocks(e: &mut Editor) -> Vec<super::super::view::Block> {
    views(e).into_iter().filter_map(|v| if let AffordanceView::Block(b) = v { Some(*b) } else { None }).collect()
}

fn block(e: &mut Editor, label: &str) -> super::super::view::Block {
    let all = blocks(e);
    all.iter().find(|b| b.label == label).cloned().unwrap_or_else(|| panic!("{label} in {:?}", all.iter().map(|b| &b.label).collect::<Vec<_>>()))
}

fn activations(e: &mut Editor) -> Vec<(String, Option<String>)> {
    e.take_notices()
        .into_iter()
        .filter_map(|n| if let super::super::input::Notice::Placeholder { key, action } = n { Some((key, action)) } else { None })
        .collect()
}

#[test]
fn a_look_rule_sets_a_blocks_colour_label_icon_hint_and_buttons() {
    use super::super::view::BlockState;
    let mut e = signing();
    let sig = block(&mut e, "Signature");
    assert_eq!((sig.base, sig.hint.as_str(), sig.icon.is_some(), sig.state), (Some(0x2E7D32), "Click to sign", true, BlockState::Rest));
    let initials = block(&mut e, "Applicant initials");
    assert_eq!((initials.base, initials.hint.as_str(), initials.icon.is_some()), (None, "Click to add", false), "the default look");
    assert_eq!(blocks(&mut e).len(), 4, "signature, initials, fingerprint and photo");
    // The press that selects a block with buttons doesn't activate it: its buttons show.
    e.take_notices();
    press(&mut e, sig.page, sig.rect);
    let sig = block(&mut e, "Signature");
    assert_eq!(sig.state, BlockState::Selected);
    assert!(activations(&mut e).is_empty());
    assert_eq!(sig.buttons.iter().map(|b| (b.id.as_str(), b.primary)).collect::<Vec<_>>(), vec![("sign", true), ("upload", false)]);
    press(&mut e, sig.page, sig.buttons[1].rect);
    assert_eq!(activations(&mut e), vec![("Signature.Applicant".to_string(), Some("upload".to_string()))]);
    // Enter presses the primary button.
    press(&mut e, sig.page, sig.rect);
    e.take_notices();
    e.execute(Command::InsertParagraph, fonts());
    assert_eq!(activations(&mut e), vec![("Signature.Applicant".to_string(), Some("sign".to_string()))]);
}

#[test]
fn a_block_without_buttons_activates_on_a_press_and_an_image_look_keeps_its_buttons() {
    use crate::api::command::{PlaceholderAction, PlaceholderLook, PlaceholderLookRule, PlaceholderLooks, PlaceholderMatch};
    let mut e = signing();
    let initials = block(&mut e, "Applicant initials");
    e.take_notices();
    press(&mut e, initials.page, initials.rect);
    assert_eq!(activations(&mut e), vec![("Initials.Applicant".to_string(), None)]);
    let picture = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let rule = PlaceholderLookRule {
        matches: PlaceholderMatch::Prefix { prefix: "Photo".into() },
        look: PlaceholderLook::Image { image: crate::api::value::Bytes(picture), mime: "image/png".into() },
        hint: None,
        actions: Some(vec![PlaceholderAction { id: "capture".into(), label: "Capture".into(), primary: Some(true) }]),
    };
    e.execute(Command::SetPlaceholderLooks(PlaceholderLooks { looks: vec![rule], merge: Some(true) }), fonts());
    let photo = block(&mut e, "Applicant photo");
    assert!(photo.image.is_some());
    press(&mut e, photo.page, photo.rect);
    assert_eq!(block(&mut e, "Applicant photo").buttons.len(), 1);
}

#[test]
fn a_filled_block_is_done_with_its_date_and_replace() {
    use super::super::view::BlockState;
    let mut e = signing();
    let picture = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let out = e.execute(
        Command::FillPlaceholder(crate::api::command::FillPlaceholder {
            key: Some("Signature.Applicant".into()),
            image: crate::api::value::Bytes(picture),
        }),
        fonts(),
    );
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    e.complete(fonts());
    let sig = block(&mut e, "Signature");
    assert_eq!((sig.state, sig.done.is_none()), (BlockState::Done, true), "no tab at rest");
    let (x, y) = e.stack().to_doc(sig.page, (sig.rect[0] + sig.rect[2]) / 2.0, (sig.rect[1] + sig.rect[3]) / 2.0).expect("on the page");
    e.pointer_input(super::super::input::PointerKind::Move, x, y, 0, super::super::keymap::Mods::default(), fonts());
    let tab = block(&mut e, "Signature").done.expect("the tab while hovered");
    assert!(tab.text.contains(&super::super::affordances::day_for_test(crate::docx::now_utc())) && tab.text.ends_with("Replace"), "{}", tab.text);
    e.take_notices();
    press(&mut e, sig.page, [tab.rect[2] - 4.0, tab.rect[1], tab.rect[2] - 1.0, tab.rect[3]]);
    assert_eq!(activations(&mut e), vec![("Signature.Applicant".to_string(), Some("replace".to_string()))]);
}

#[test]
fn with_the_affordances_off_placeholders_draw_as_before() {
    use crate::api::command::{PlaceholderLook, PlaceholderLookRule, PlaceholderLooks, PlaceholderMatch};
    let mut e = signing();
    let look = PlaceholderLook::Drawn { fill: Some(crate::api::value::Color(0x0A74E4)), label: None, icon_svg: None };
    let rule =
        PlaceholderLookRule { matches: PlaceholderMatch::Default(crate::api::command::DefaultKeyword::Default), look, hint: None, actions: None };
    e.execute(Command::SetPlaceholderLooks(PlaceholderLooks { looks: vec![rule], merge: Some(true) }), fonts());
    let n = pages(&mut e);
    assert!(e.view(0..n).placeholders.is_empty(), "blocks draw them while on");
    e.view_settings.placeholder_ui = false;
    let n = pages(&mut e);
    let view = e.view(0..n);
    assert!(view.affordances.is_empty());
    assert!(!view.placeholders.is_empty(), "today's placeholder looks");
}

#[test]
fn unmapped_fields_are_drawn_neutral_until_cleared() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let state = |e: &mut Editor| marks(e).into_iter().find(|m| m.0 == "Applicant.Email").map(|m| m.3);
    e.execute(Command::SetUnmappedFields(crate::api::command::UnmappedFields { names: vec!["Applicant.Email".into()] }), fonts());
    assert_eq!(state(&mut e), Some(FieldState::Unmapped));
    let t = target_with(&mut e, "Applicant.Email");
    assert!(e.describe(t).is_some_and(|d| d.unmapped));
    select(&mut e, t);
    assert_eq!(state(&mut e), Some(FieldState::Selected), "selected wins");
    e.execute(Command::SetUnmappedFields(crate::api::command::UnmappedFields { names: Vec::new() }), fonts());
    assert_eq!(state(&mut e), Some(FieldState::Selected));
    assert!(!e.describe(t).is_some_and(|d| d.unmapped), "cleared");
}

#[test]
fn an_if_is_tinted_and_shows_its_condition_only_while_hovered() {
    let mut e = design("FX-PLACEHOLDER-UI.docx");
    let ifs = |e: &mut Editor| {
        views(e)
            .into_iter()
            .filter_map(|v| if let AffordanceView::IfRegion { target, tab, rects, .. } = v { Some((target, tab, rects)) } else { None })
            .collect::<Vec<_>>()
    };
    let all = ifs(&mut e);
    let (target, tab, rects) = all.first().cloned().expect("the IF");
    assert!(tab.is_none() && !rects.is_empty(), "no tab at rest");
    e.affordances.hovered = Some(target);
    let tab = ifs(&mut e).into_iter().find(|i| i.0 == target).and_then(|i| i.1).expect("its tab while hovered");
    assert_eq!(tab.text, "IF {Fee} > 0 \u{b7} else: nothing");
    assert!(tab.rect[3] <= rects[0][1] + 0.01, "above the region");
}
