//! Floating objects, pictures and groups

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::position::{Position, StoryId};
use super::super::selection::{ObjectRef, Selection};
use super::{fonts, open};
use crate::api::command::{AlignObjects, AlignReference, AltText, ObjectAlign, ObjectSize, Picture, PictureCrop};
use crate::api::drawing::{Arrange, ShapeInsert, ShapePreset};
use crate::api::value::{Bytes, ObjectWrap, Points};
use crate::docx::{Block, Inline};
use crate::layout::tests::shapes::{PT, anchor, bytes, sp};
use crate::layout::{FloatId, Unit};

const NONE: Mods = Mods { ctrl: false, shift: false, alt: false, meta: false };
const ALT: Mods = Mods { ctrl: false, shift: false, alt: true, meta: false };
const SHIFT: Mods = Mods { ctrl: false, shift: true, alt: false, meta: false };
const WP14: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing";

fn editor(body: &str) -> Editor {
    let mut e = Editor::open(&bytes(body).expect("a package"), fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn press(e: &mut Editor, x: f64, y: f64, m: Mods) {
    let (dx, dy) = e.stack().to_doc(0, x, y).expect("on the stack");
    e.pointer_input(PointerKind::Down, dx, dy, 1, m, fonts());
    e.pointer_input(PointerKind::Up, dx, dy, 0, m, fonts());
}

fn drag(e: &mut Editor, from: [f64; 2], to: [f64; 2], m: Mods) {
    let doc = |e: &Editor, p: [f64; 2]| e.stack().to_doc(0, p[0], p[1]).expect("on the stack");
    let (a, b) = (doc(e, from), doc(e, to));
    e.pointer_input(PointerKind::Down, a.0, a.1, 1, m, fonts());
    for k in 1..=8 {
        let t = f64::from(k) / 8.0;
        e.pointer_input(PointerKind::Move, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, 0, m, fonts());
    }
    e.pointer_input(PointerKind::Up, b.0, b.1, 0, m, fonts());
    e.complete(fonts());
}

fn all_floats(e: &Editor) -> Vec<u64> {
    let mut v = Vec::new();
    for s in &e.document().sections {
        for b in &s.blocks {
            if let Block::Para(p) = b {
                v.extend(p.inlines.iter().filter_map(|i| if let Inline::Float(f, _) = i { Some(f.id) } else { None }));
            }
        }
    }
    v
}

fn selected_picture(e: &Editor) -> Option<&crate::docx::Picture> {
    let Selection::Object(ObjectRef::Inline { para, offset }) = e.selection() else { return None };
    match super::super::stories::inline_at(e.index().para(e.document(), para)?, offset)? {
        Inline::Picture(p, _) => Some(p),
        _ => None,
    }
}

fn first_inline_picture(e: &Editor) -> Option<&crate::docx::Picture> {
    e.document().sections.iter().flat_map(|s| s.blocks.iter()).find_map(|b| match b {
        Block::Para(p) => p.inlines.iter().find_map(|i| if let Inline::Picture(p, _) = i { Some(p) } else { None }),
        _ => None,
    })
}

/// The shapes drawn on page 0 as (rect in tenths of a point, rotation in tenths of a degree, flips), sorted.
fn drawn(e: &mut Editor) -> Vec<([i64; 4], i64, bool, bool)> {
    e.complete(fonts());
    let snap = e.snapshot();
    let mut v: Vec<_> = crate::layout::traverse::paint_items(&snap.pages()[0])
        .iter()
        .filter_map(|it| match it {
            crate::layout::fragment::Item::Shape(s) => {
                Some((s.rect.map(|v| (v * 10.0).round() as i64), (s.rot.rem_euclid(360.0) * 10.0).round() as i64 % 3600, s.flip_h, s.flip_v))
            }
            _ => None,
        })
        .collect();
    v.sort();
    v
}

/// The first section's `i`th block, a paragraph.
fn body_para(e: &Editor, i: usize) -> crate::layout::ParaId {
    match &e.document().sections[0].blocks[i] {
        Block::Para(p) => crate::layout::ParaId(p.id),
        Block::Table(_) => panic!("a table"),
    }
}

fn select(e: &mut Editor, f: u64) {
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(f))));
    e.complete(fonts());
}

fn changed(out: &Outcome) -> bool {
    matches!(out, Outcome::Changed(_))
}

/// A blue square anchored at (72, 300) and placed as `position` gives it.
fn placed(position: &str) -> Editor {
    let s = sp(0, 0, 100, 100, "rect", "", "0000FF");
    let a = anchor(72, 300, 100, 100, 1, false, 1, &s)
        .replace(r#"<wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH>"#, position);
    editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{a}</w:p>"))
}

#[test]
fn aligning_a_percent_positioned_float_moves_it() {
    let mut e =
        placed(&format!(r#"<wp:positionH relativeFrom="margin"><wp14:pctPosHOffset xmlns:wp14="{WP14}">10000</wp14:pctPosHOffset></wp:positionH>"#));
    let f = all_floats(&e)[0];
    select(&mut e, f);
    let before = drawn(&mut e)[0].0;
    let out = e.execute(Command::AlignObjects(AlignObjects { align: ObjectAlign::Right, relative_to: Some(AlignReference::Page) }), fonts());
    assert!(changed(&out), "{out:?}");
    let after = drawn(&mut e)[0].0;
    assert_eq!(after[2], 6120, "the right edge on the page's: {before:?} → {after:?}");
}

#[test]
fn a_typed_width_replaces_a_relative_one() {
    let s = sp(0, 0, 100, 100, "rect", "", "0000FF");
    let a = anchor(72, 300, 100, 100, 1, false, 1, &s).replace(
        "<a:graphic>",
        &format!(r#"<wp14:sizeRelH xmlns:wp14="{WP14}" relativeFrom="page"><wp14:pctWidth>50000</wp14:pctWidth></wp14:sizeRelH><a:graphic>"#),
    );
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{a}</w:p>"));
    let f = all_floats(&e)[0];
    select(&mut e, f);
    let before = drawn(&mut e)[0].0;
    assert_eq!(before[2] - before[0], 3060, "half the page: {before:?}");
    let out = e.execute(Command::SetObjectSize(ObjectSize { width: Some(Points(100.0)), ..Default::default() }), fonts());
    assert!(changed(&out), "{out:?}");
    let after = drawn(&mut e)[0].0;
    assert_eq!(after[2] - after[0], 1000, "{after:?}");
}

#[test]
fn a_dragged_size_replaces_a_relative_one() {
    let s = sp(0, 0, 100, 100, "rect", "", "0000FF");
    let a = anchor(72, 300, 100, 100, 1, false, 1, &s).replace(
        "<a:graphic>",
        &format!(r#"<wp14:sizeRelH xmlns:wp14="{WP14}" relativeFrom="page"><wp14:pctWidth>50000</wp14:pctWidth></wp14:sizeRelH><a:graphic>"#),
    );
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{a}</w:p>"));
    let f = FloatId(all_floats(&e)[0]);
    let (page, r) = e.float_box(f, fonts()).expect("drawn");
    let out = e.place_float(f, page, r, [r[0], r[1], r[0] + 50.0, r[3]], "Resize Object", fonts());
    assert!(changed(&out), "{out:?}");
    let after = drawn(&mut e)[0].0;
    assert_eq!(after[2] - after[0], 500, "{after:?}");
}

#[test]
fn resizing_an_aligned_float_keeps_its_fixed_edges() {
    for (align, fixed) in [("right", false), ("center", false), ("left", true)] {
        let mut e = placed(&format!(r#"<wp:positionH relativeFrom="margin"><wp:align>{align}</wp:align></wp:positionH>"#));
        let f = FloatId(all_floats(&e)[0]);
        let (page, r) = e.float_box(f, fonts()).expect("drawn");
        let out = e.place_float(f, page, r, [r[0], r[1], r[2] + 30.0, r[3]], "Resize Object", fonts());
        assert!(changed(&out), "{out:?}");
        let after = drawn(&mut e)[0].0;
        assert_eq!((after[0], after[2] - after[0]), ((r[0] * 10.0).round() as i64, 1300), "{align}: {r:?} → {after:?}");
        let n = e.float_of(f).expect("the float");
        assert_eq!(n.h_align.is_some(), fixed, "{align}: {:?}", n.h_align);
    }
}

#[test]
fn moving_a_column_float_in_a_continuous_section_uses_its_columns() {
    let s = sp(0, 0, 100, 100, "rect", "", "0000FF");
    let a = anchor(72, 300, 100, 100, 1, false, 1, &s).replace(
        r#"<wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH>"#,
        r#"<wp:positionH relativeFrom="column"><wp:align>center</wp:align></wp:positionH>"#,
    );
    let first = r#"<w:p><w:pPr><w:sectPr><w:type w:val="nextPage"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:pPr><w:r><w:t>One</w:t></w:r></w:p>"#;
    let body = format!(r#"{first}<w:p><w:r><w:t>Two</w:t></w:r><w:r><w:br w:type="column"/></w:r></w:p><w:p><w:r><w:t>Three</w:t></w:r>{a}</w:p>"#);
    let b = with_document(&bytes(&body).expect("a package"), |d| {
        d.replace("<w:sectPr><w:pgSz", r#"<w:sectPr><w:type w:val="continuous"/><w:cols w:num="2" w:space="720"/><w:pgSz"#)
    });
    let mut e = Editor::open(&b, fonts()).expect("opens");
    e.complete(fonts());
    assert_eq!(e.document().sections.len(), 2);
    assert_eq!(e.snapshot().pages().len(), 1, "one page");
    let f = FloatId(all_floats(&e)[0]);
    let (page, r) = e.float_box(f, fonts()).expect("drawn");
    assert!(r[0] > 306.0, "in the second column: {r:?}");
    let out = e.place_float(f, page, r, [r[0] + 10.0, r[1], r[2] + 10.0, r[3]], "Move Object", fonts());
    assert!(changed(&out), "{out:?}");
    let after = drawn(&mut e)[0].0;
    assert!((after[0] - (r[0] * 10.0).round() as i64 - 100).abs() <= 5, "moved by 10pt: {r:?} → {after:?}");
}

#[test]
fn grouping_floats_anchored_a_b_a_makes_one_group() {
    let r = |x, y, id| anchor(x, y, 60, 60, id, false, id, &sp(0, 0, 60, 60, "rect", "", "FF0000"));
    let body = format!("<w:p><w:r><w:t>A</w:t></w:r>{}{}</w:p><w:p><w:r><w:t>B</w:t></w:r>{}</w:p>", r(72, 300, 1), r(72, 450, 3), r(300, 300, 2));
    let mut e = editor(&body);
    assert_eq!(all_floats(&e).len(), 3);
    press(&mut e, 100.0, 330.0, NONE);
    press(&mut e, 330.0, 330.0, SHIFT);
    press(&mut e, 100.0, 480.0, SHIFT);
    assert_eq!(e.selected_floats().len(), 3, "{:?}", e.selection());
    let out = e.group_objects(fonts());
    assert!(changed(&out), "{out:?}");
    assert_eq!(all_floats(&e).len(), 1, "one group: {:?}", all_floats(&e));
}

/// A star and an ellipse side by side.
fn two_shapes() -> Editor {
    let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
    let ellipse = sp(0, 0, 100, 60, "ellipse", "", "00FF00");
    editor(&format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}{}</w:p>",
        anchor(100, 300, 100, 100, 1, false, 5, &star),
        anchor(260, 340, 100, 60, 2, false, 6, &ellipse)
    ))
}

fn select_both(e: &mut Editor) {
    press(e, 150.0, 350.0, NONE);
    press(e, 310.0, 370.0, SHIFT);
    assert_eq!(e.selected_floats().len(), 2);
}

fn ungroup_keeps_the_drawing(deg: f64, flip: bool) {
    let mut e = two_shapes();
    select_both(&mut e);
    assert!(changed(&e.group_objects(fonts())));
    assert!(changed(&e.set_object_rotation(deg, fonts())));
    if flip {
        assert!(changed(&e.execute(Command::FlipObject(crate::api::drawing::FlipAxis::Horizontal), fonts())));
    }
    let was = drawn(&mut e);
    let out = e.ungroup_objects(fonts());
    assert!(changed(&out), "{out:?}");
    let now = drawn(&mut e);
    assert_eq!(now.len(), was.len());
    for (a, b) in now.iter().zip(&was) {
        assert!(a.0.iter().zip(&b.0).all(|(p, q)| (p - q).abs() <= 1) && (a.1, a.2, a.3) == (b.1, b.2, b.3), "{now:?} vs {was:?}");
    }
}

#[test]
fn ungrouping_a_turned_group_keeps_its_children_where_they_are_drawn() {
    ungroup_keeps_the_drawing(30.0, false);
}

#[test]
fn ungrouping_a_quarter_turned_group_keeps_its_children_where_they_are_drawn() {
    ungroup_keeps_the_drawing(90.0, false);
}

#[test]
fn ungrouping_a_flipped_group_keeps_its_children_where_they_are_drawn() {
    ungroup_keeps_the_drawing(30.0, true);
}

#[test]
fn delete_removes_every_selected_float_in_one_step() {
    let mut e = two_shapes();
    select_both(&mut e);
    e.execute(Command::DeleteObject, fonts());
    assert_eq!(all_floats(&e).len(), 0);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(all_floats(&e).len(), 2, "one undo restores both");
}

#[test]
fn deleting_selected_floats_needs_each_anchor_editable() {
    use crate::api::command::{DeleteOptions, DocumentProtection, ProtectionKind};
    let a = anchor(100, 300, 100, 100, 1, false, 5, &sp(0, 0, 100, 100, "rect", "", "FF0000"));
    let b = anchor(260, 340, 100, 60, 2, false, 6, &sp(0, 0, 100, 60, "ellipse", "", "00FF00"));
    let open = |x: &str| format!(r#"<w:p><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>In</w:t></w:r>{x}<w:permEnd w:id="1"/></w:p>"#);
    let apart = format!("{}<w:p><w:r><w:t>Out</w:t></w:r>{b}</w:p>", open(&a));
    let together = open(&format!("{a}{b}"));
    let delete = Command::Delete(DeleteOptions { direction: crate::api::value::Direction::Forward, unit: None });
    // Object commands stay refused in an editable range; the text Delete deletes what's selected there.
    for (body, deletes, commands) in [(&apart, false, vec![Command::DeleteObject, delete.clone()]), (&together, true, vec![delete])] {
        for command in commands {
            let mut e = editor(body);
            let read_only = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
            assert!(changed(&e.execute(Command::SetDocumentProtection(read_only), fonts())));
            let fs: Vec<FloatId> = all_floats(&e).into_iter().map(FloatId).collect();
            e.set_selection(Selection::Object(ObjectRef::Floating(fs[0])));
            e.object_also = Some((fs[0], vec![fs[1]]));
            let out = e.execute(command.clone(), fonts());
            assert_eq!(changed(&out), deletes, "{command:?}: {out:?}");
            assert_eq!(all_floats(&e).len(), if deletes { 0 } else { 2 }, "{command:?}");
            if deletes {
                e.execute(Command::Undo(Default::default()), fonts());
                assert_eq!(all_floats(&e).len(), 2, "one undo restores both");
            }
        }
    }
}

#[test]
fn deleting_a_stale_group_child_keeps_the_group() {
    let group = format!(
        r#"<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{w}"/><a:chOff x="0" y="0"/><a:chExt cx="{w}" cy="{w}"/></a:xfrm></wpg:grpSpPr>{}</wpg:wgp>"#,
        sp(0, 0, 80, 80, "rect", "", "0000FF"),
        w = 80 * PT,
    );
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(72, 300, 80, 80, 1, false, 1, &group)));
    let f = FloatId(all_floats(&e)[0]);
    select(&mut e, f.0);
    e.object_child = Some((f, vec![1]));
    let out = e.execute(Command::DeleteObject, fonts());
    assert!(!changed(&out), "{out:?}");
    assert_eq!(all_floats(&e), vec![f.0], "the group stays");
}

#[test]
fn delete_removes_only_the_selected_group_child() {
    let group = format!(
        r#"<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{h}"/><a:chOff x="0" y="0"/><a:chExt cx="{w}" cy="{h}"/></a:xfrm></wpg:grpSpPr>{}{}</wpg:wgp>"#,
        sp(0, 0, 80, 80, "rect", "", "0000FF"),
        sp(120, 0, 80, 80, "rect", "", "00FF00"),
        w = 200 * PT,
        h = 100 * PT,
    );
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(72, 300, 200, 100, 1, false, 1, &group)));
    press(&mut e, 100.0, 330.0, NONE);
    press(&mut e, 100.0, 330.0, NONE);
    assert_eq!(e.selected_child(), Some(&[0u16][..]));
    let green = drawn(&mut e).into_iter().find(|s| s.0[0] > 72 * 10 + 100 * 10).expect("the second child");
    e.execute(Command::DeleteObject, fonts());
    assert_eq!(all_floats(&e).len(), 1, "the group stays");
    let f = e.float_of(FloatId(all_floats(&e)[0])).cloned().expect("the group");
    assert_eq!(f.shape.as_ref().and_then(|s| s.group().map(|g| g.children.len())), Some(1));
    assert_eq!(drawn(&mut e), vec![green], "the other child stays where it is drawn");
    assert!((f.w - 80.0).abs() < 0.1, "the group fits what is left: {}", f.w);
}

/// The blank document with a picture inserted in its first paragraph.
fn with_inserted_picture() -> Editor {
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut e = Editor::open(&super::blank_with("Body text here"), fonts()).expect("opens");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 0), Unit(2))));
    let out = e.execute(Command::InsertPicture(Picture { data: Bytes(data), alt_text: None }), fonts());
    assert!(changed(&out), "{out:?}");
    e
}

#[test]
fn an_inserted_picture_can_be_cropped() {
    let mut e = with_inserted_picture();
    let w = first_inline_picture(&e).expect("the picture").w;
    let out = e.execute(Command::SetPictureCrop(PictureCrop { left: Some(0.25), ..Default::default() }), fonts());
    assert!(changed(&out), "{out:?}");
    let p = first_inline_picture(&e).expect("the picture");
    assert!((p.w - w * 0.75).abs() < 0.1, "{} of {w}", p.w);
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    assert!(xml.contains("<a:srcRect l=\"25000\""), "the crop is saved");
    Editor::open(&saved, fonts()).expect("reopens");
}

#[test]
fn a_pictures_adjustable_geometry_is_offered_for_adjusting() {
    use crate::api::command::CommandId;
    let enabled = |e: &mut Editor| {
        let f = e.format_state();
        e.command_states(f.as_ref(), super::super::state::StateFlags { read_only: false })[&CommandId::SetShapeAdjust].enabled
    };
    let mut e = inline_picture(200, 100, "", "", "roundRect", "");
    assert!(enabled(&mut e), "a rounded picture's corners adjust");
    assert!(changed(&e.set_shape_adjust(0, 30000, fonts())));
    let mut e = inline_picture(200, 100, "", "", "rect", "");
    assert!(!enabled(&mut e), "a plain picture has nothing to adjust");
}

#[test]
fn crop_is_offered_for_pictures_only() {
    use crate::api::command::CommandId;
    let enabled = |e: &mut Editor| {
        let f = e.format_state();
        e.command_states(f.as_ref(), super::super::state::StateFlags { read_only: false })[&CommandId::SetPictureCrop].enabled
    };
    let mut e = with_inserted_picture();
    assert!(enabled(&mut e), "a picture crops");
    let mut e = Editor::open(&bytes("<w:p><w:r><w:t>Text</w:t></w:r></w:p>").expect("a package"), fonts()).expect("opens");
    e.execute(Command::InsertShape(ShapeInsert { preset: ShapePreset::from_name("rect").unwrap(), width: None, height: None }), fonts());
    assert!(!enabled(&mut e), "a shape doesn't");
}

#[test]
fn an_inserted_picture_can_be_rotated_once_floating() {
    let mut e = with_inserted_picture();
    assert!(changed(&e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts())));
    let out = e.execute(Command::SetObjectRotation(90.0), fonts());
    assert!(changed(&out), "rotate: {out:?}");
}

#[test]
fn a_quarter_turned_new_picture_saves_its_unturned_extent() {
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut doc = crate::docx::read_editable(&super::blank_with("Body text here")).unwrap();
    doc.settings.policy.mode = 15;
    let mut e = Editor::with_document(doc, fonts());
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 0), Unit(2))));
    assert!(changed(&e.execute(Command::InsertPicture(Picture { data: Bytes(data), alt_text: None }), fonts())));
    assert!(changed(&e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts())));
    assert!(changed(&e.execute(Command::SetObjectRotation(90.0), fonts())));
    let (w, h, swap) = e.document().sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| &p.inlines)
        .find_map(|i| if let crate::docx::Inline::Float(f, _) = i { Some((f.w, f.h, f.rot_swap)) } else { None })
        .expect("the float");
    assert!(swap && (w - h).abs() > 1.0, "a quarter-turned, unsquare box: {w} x {h}");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    let extent = format!(r#"<wp:extent cx="{}" cy="{}""#, (h * 12700.0).round() as i64, (w * 12700.0).round() as i64);
    assert!(xml.contains(&extent), "{extent} in {xml}");
}

#[test]
fn an_inserted_picture_can_be_marked_decorative() {
    let mut e = with_inserted_picture();
    let out = e.execute(Command::SetAltText(AltText { text: String::new(), decorative: Some(true) }), fonts());
    assert!(changed(&out), "{out:?}");
    assert!(first_inline_picture(&e).and_then(|p| p.shape.as_ref()).is_some_and(|s| s.nv.decorative));
}

/// A section with a half-inch gutter.
fn with_gutter(src: &[u8]) -> Vec<u8> {
    with_document(src, |d| d.replace(r#"w:gutter="0""#, r#"w:gutter="720""#))
}

/// The package with its main part edited.
fn with_document(src: &[u8], edit: impl Fn(String) -> String) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "word/document.xml" {
            data = edit(String::from_utf8(data).unwrap()).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

#[test]
fn a_float_centred_in_a_gutter_section_moves_by_the_drag() {
    let s = sp(0, 0, 100, 100, "rect", "", "0000FF");
    let a = anchor(72, 300, 100, 100, 1, false, 1, &s).replace(
        r#"<wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH>"#,
        r#"<wp:positionH relativeFrom="margin"><wp:align>center</wp:align></wp:positionH>"#,
    );
    let b = with_gutter(&bytes(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{a}</w:p>")).expect("a package"));
    let mut e = Editor::open(&b, fonts()).expect("opens");
    e.complete(fonts());
    let f = all_floats(&e)[0];
    select(&mut e, f);
    let before = drawn(&mut e)[0].0;
    let (cx, cy) = ((before[0] + before[2]) as f64 / 20.0, (before[1] + before[3]) as f64 / 20.0);
    drag(&mut e, [cx, cy], [cx + 10.0, cy], ALT);
    let after = drawn(&mut e)[0].0;
    assert!((after[0] - before[0] - 100).abs() <= 5, "moved by 10pt: {before:?} → {after:?}");
}

#[test]
fn a_float_aligned_to_its_character_moves_by_the_drag() {
    let mut e = placed(r#"<wp:positionH relativeFrom="character"><wp:align>left</wp:align></wp:positionH>"#);
    let f = all_floats(&e)[0];
    select(&mut e, f);
    let before = drawn(&mut e)[0].0;
    let (cx, cy) = ((before[0] + before[2]) as f64 / 20.0, (before[1] + before[3]) as f64 / 20.0);
    drag(&mut e, [cx, cy], [cx + 10.0, cy], ALT);
    let after = drawn(&mut e)[0].0;
    assert!((after[0] - before[0] - 100).abs() <= 5, "moved by 10pt: {before:?} → {after:?}");
}

#[test]
fn snap_margins_on_an_even_mirrored_page_are_its_drawn_margins() {
    use crate::api::command::{MultiplePages, MultiplePagesMode};
    let body = r#"<w:p><w:r><w:t>One</w:t></w:r><w:r><w:br w:type="page"/></w:r><w:r><w:t>Two</w:t></w:r></w:p>"#;
    let b = with_gutter(&bytes(body).expect("a package"));
    let mut e = Editor::open(&b, fonts()).expect("opens");
    let mirror = MultiplePages { mode: Some(MultiplePagesMode::MirrorMargins), gutter_at_top: Some(false) };
    assert!(changed(&e.execute(Command::SetMultiplePages(mirror), fonts())));
    e.complete(fonts());
    assert!(e.snapshot().pages().len() >= 2);
    let s = e.document().sections[0].clone();
    e.build_snaps(1, super::super::objects::Dragged::Float(FloatId(u64::MAX)));
    let w = e.snapshot().pages()[1].w;
    assert_eq!([e.snaps.margins[0], e.snaps.margins[2]], [s.right, w - s.left - s.gutter], "the inside margin and its gutter on the right");
}

fn select_first_inline_picture(e: &mut Editor) {
    let (para, offset) = e
        .index()
        .para_ids()
        .find_map(|id| {
            let p = e.index().para(e.document(), id)?;
            let mut at = 0u32;
            p.inlines.iter().find_map(|i| {
                let here = at;
                at += i.units() as u32;
                matches!(i, Inline::Picture(..)).then_some((id, Unit(here)))
            })
        })
        .expect("an inline picture");
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Inline { para, offset }));
}

/// The fixture's first inline picture, wrapped square.
fn float_picture() -> (Editor, FloatId) {
    let mut e = open("FX-OBJ.docx");
    select_first_inline_picture(&mut e);
    let out = e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts());
    assert!(changed(&out), "{out:?}");
    let Selection::Object(ObjectRef::Floating(f)) = e.selection() else { panic!("{:?}", e.selection()) };
    (e, f)
}

fn natural(e: &Editor, f: &crate::docx::Float) -> (f64, f64) {
    let (w, h, dx, dy) = crate::docx::image::image_info(&e.document().images[f.image.expect("image")]).expect("decodes");
    (w * 72.0 / dx, h * 72.0 / dy)
}

#[test]
fn a_crop_on_a_quarter_turned_float_scales_the_pictures_own_width() {
    let (mut e, f) = float_picture();
    assert!(changed(&e.execute(Command::SetObjectRotation(90.0), fonts())));
    let n = e.float_of(f).cloned().expect("float");
    assert!(n.rot_swap);
    let out = e.execute(Command::SetPictureCrop(PictureCrop { left: Some(0.5), ..Default::default() }), fonts());
    assert!(changed(&out), "{out:?}");
    let m = e.float_of(f).cloned().expect("float");
    assert!((m.w - n.w).abs() < 0.1, "box width unchanged: {} → {}", n.w, m.w);
    assert!((m.h - n.h / 2.0).abs() < 0.1, "box height halves: {} → {}", n.h, m.h);
}

#[test]
fn reset_size_on_a_quarter_turned_float_swaps_the_natural_size() {
    let (mut e, f) = float_picture();
    assert!(changed(&e.execute(Command::SetObjectRotation(90.0), fonts())));
    e.execute(Command::ResetObjectSize, fonts());
    let n = e.float_of(f).cloned().expect("float");
    let (nw, nh) = natural(&e, &n);
    assert!((n.w - nh).abs() < 0.1 && (n.h - nw).abs() < 0.1, "({}, {}) vs natural ({nw}, {nh})", n.w, n.h);
}

#[test]
fn reset_size_keeps_the_shown_share_of_a_cropped_picture() {
    let (mut e, f) = float_picture();
    assert!(changed(&e.execute(Command::SetPictureCrop(PictureCrop { left: Some(0.5), ..Default::default() }), fonts())));
    e.execute(Command::ResetObjectSize, fonts());
    let n = e.float_of(f).cloned().expect("float");
    let (nw, nh) = natural(&e, &n);
    assert!((n.w - nw * 0.5).abs() < 0.1 && (n.h - nh).abs() < 0.1, "({}, {}) vs natural ({nw}, {nh}) with half its width shown", n.w, n.h);
}

#[test]
fn bring_to_front_raises_floats_the_editor_made() {
    let mut e = Editor::open(&bytes("<w:p><w:r><w:t>Text</w:t></w:r></w:p>").expect("a package"), fonts()).expect("opens");
    e.execute(Command::InsertText("Shapes".into()), fonts());
    let shape = || Command::InsertShape(ShapeInsert { preset: ShapePreset::from_name("star5").unwrap(), width: None, height: None });
    e.execute(shape(), fonts());
    e.execute(Command::Escape, fonts());
    e.execute(shape(), fonts());
    let ids = all_floats(&e);
    assert_eq!(ids.len(), 2, "{ids:?}");
    let z = |e: &Editor, f| e.float_of(FloatId(f)).map(|n| n.z).unwrap();
    let first = if z(&e, ids[0]) < z(&e, ids[1]) { ids[0] } else { ids[1] };
    let other = if first == ids[0] { ids[1] } else { ids[0] };
    select(&mut e, first);
    let out = e.arrange_object(Arrange::ToFront, fonts());
    assert!(changed(&out), "{out:?}");
    assert!(z(&e, first) > z(&e, other));
    let out = e.arrange_object(Arrange::Backward, fonts());
    assert!(changed(&out), "{out:?}");
    assert!(z(&e, first) < z(&e, other));
}

#[test]
fn floating_objects_are_refused_in_a_footnote() {
    let mut e = Editor::open(&super::blank_with("Body text here"), fonts()).expect("opens");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 0), Unit(2))));
    e.execute(Command::InsertFootnote, fonts());
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(StoryId::Footnote(_))), "{:?}", e.selection());
    let shape = Command::InsertShape(ShapeInsert { preset: ShapePreset::from_name("star5").unwrap(), width: None, height: None });
    for c in [shape, Command::InsertTextBox] {
        let out = e.execute(c, fonts());
        assert!(matches!(out, Outcome::Refused(_)), "{out:?}");
    }
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    assert!(changed(&e.execute(Command::InsertPicture(Picture { data: Bytes(data), alt_text: None }), fonts())));
    assert!(matches!(e.selection(), Selection::Object(ObjectRef::Inline { .. })), "{:?}", e.selection());
    let out = e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts());
    assert!(matches!(out, Outcome::Refused(_)), "{out:?}");
    let floats =
        e.document().footnotes.values().flatten().any(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| matches!(i, Inline::Float(..)))));
    assert!(!floats, "no float went in the note");
}

#[test]
fn floating_and_inlining_a_picture_keeps_its_effect_extent() {
    fn give(blocks: &mut [Block], para: u64, effect: [f64; 4]) {
        for b in blocks {
            match b {
                Block::Para(p) if p.id == para => p.inlines.iter_mut().for_each(|i| {
                    if let Inline::Picture(pic, _) = i {
                        pic.effect = effect;
                    }
                }),
                Block::Para(_) => {}
                Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| give(&mut c.blocks, para, effect)),
            }
        }
    }
    let mut e = open("FX-OBJ.docx");
    let effect = [1.0, 2.0, 3.0, 4.0];
    select_first_inline_picture(&mut e);
    let Selection::Object(ObjectRef::Inline { para, .. }) = e.selection() else { panic!("{:?}", e.selection()) };
    e.doc.sections.iter_mut().for_each(|s| give(&mut s.blocks, para.0, effect));
    assert!(changed(&e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts())));
    let Selection::Object(ObjectRef::Floating(f)) = e.selection() else { panic!("{:?}", e.selection()) };
    assert_eq!(e.float_of(f).map(|n| n.effect), Some(effect), "floated");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    assert!(xml.contains(r#"<wp:effectExtent l="12700" t="25400" r="38100" b="50800"/>"#), "the floated picture's extent is saved");
    assert!(changed(&e.execute(Command::SetObjectWrap(ObjectWrap::Inline), fonts())));
    assert_eq!(selected_picture(&e).map(|p| p.effect), Some(effect), "in line again");
}

#[test]
fn inlining_a_quarter_turned_float_keeps_the_pictures_own_size() {
    let (mut e, f) = float_picture();
    let n = e.float_of(f).cloned().expect("float");
    assert!(changed(&e.execute(Command::SetObjectRotation(90.0), fonts())));
    assert!(changed(&e.execute(Command::SetObjectWrap(ObjectWrap::Inline), fonts())));
    let p = selected_picture(&e).expect("in line");
    assert!((p.w - n.w).abs() < 0.1 && (p.h - n.h).abs() < 0.1, "({}, {}) vs ({}, {})", p.w, p.h, n.w, n.h);
}

/// The selected float's wrap polygon on page 0, in document points.
fn wrap_points(e: &mut Editor) -> Vec<[f64; 2]> {
    let w = e.wrap_points(0..1).expect("editing wrap points");
    w.points.iter().map(|&[x, y]| e.stack().to_doc(w.page, x, y).map(|(x, y)| [x, y]).expect("on the stack")).collect()
}

fn drag_doc(e: &mut Editor, from: [f64; 2], to: [f64; 2], m: Mods) {
    e.pointer_input(PointerKind::Down, from[0], from[1], 1, m, fonts());
    e.pointer_input(PointerKind::Move, to[0], to[1], 0, m, fonts());
    e.pointer_input(PointerKind::Up, to[0], to[1], 0, m, fonts());
}

fn poly(e: &Editor) -> Option<Vec<(i64, i64)>> {
    let f = FloatId(all_floats(e)[0]);
    e.float_of(f)?.poly.as_ref().map(|p| p.iter().map(|&(x, y)| ((x * 100.0).round() as i64, (y * 100.0).round() as i64)).collect())
}

#[test]
fn wrap_points_drag_add_and_remove_as_one_step_each_and_save() {
    let mut e = two_shapes();
    press(&mut e, 150.0, 350.0, NONE);
    assert!(matches!(e.edit_wrap_points(), Outcome::Moved));
    let pts = wrap_points(&mut e);
    assert_eq!(pts.len(), 4, "the box: {pts:?}");
    let depth = e.history.undo_depth();
    let [x, y] = pts[0];
    e.pointer_input(PointerKind::Move, x, y, 0, NONE, fonts());
    assert_eq!(e.view(0..1).cursor, super::super::input::Cursor::Move, "the move pointer over a vertex");
    e.pointer_input(PointerKind::Move, (pts[1][0] + pts[2][0]) / 2.0, pts[1][1], 0, NONE, fonts());
    assert_eq!(e.view(0..1).cursor, super::super::input::Cursor::Crosshair, "over an edge");
    // A vertex dragged a fifth of the way in
    drag_doc(&mut e, [x, y], [x + 20.0, y + 20.0], NONE);
    assert_eq!(e.history.undo_depth(), depth + 1, "one step");
    assert_eq!(poly(&e), Some(vec![(20, 20), (0, 100), (100, 100), (100, 0), (20, 20)]));
    let f = FloatId(all_floats(&e)[0]);
    assert_eq!(e.float_of(f).map(|f| f.wrap), Some(crate::docx::FloatWrap::Tight), "a square wrap becomes tight");
    assert!(e.editing_wrap_points(), "still editing");
    // A point added on the bottom edge
    let pts = wrap_points(&mut e);
    let mid = [(pts[1][0] + pts[2][0]) / 2.0, pts[1][1]];
    drag_doc(&mut e, mid, [mid[0], mid[1] + 10.0], NONE);
    assert_eq!(poly(&e).map(|p| p.len()), Some(6), "{:?}", poly(&e));
    assert_eq!(poly(&e).and_then(|p| p.get(2).copied()), Some((50, 110)));
    // Ctrl+click removes it
    let pts = wrap_points(&mut e);
    let ctrl = Mods { ctrl: true, ..NONE };
    e.pointer_input(PointerKind::Down, pts[2][0], pts[2][1], 1, ctrl, fonts());
    e.pointer_input(PointerKind::Up, pts[2][0], pts[2][1], 1, ctrl, fonts());
    assert_eq!(poly(&e).map(|p| p.len()), Some(5));
    assert_eq!(e.history.undo_depth(), depth + 3);
    // Saved and read again
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&saved, fonts()).expect("opens");
    assert_eq!(poly(&again), poly(&e));
    e.execute(Command::Undo(Default::default()), fonts());
    e.execute(Command::Undo(Default::default()), fonts());
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(poly(&e), None, "undone");
    // A press elsewhere ends it
    assert!(matches!(e.edit_wrap_points(), Outcome::Moved));
    let (dx, dy) = e.stack().to_doc(0, 400.0, 600.0).expect("on the stack");
    e.pointer_input(PointerKind::Down, dx, dy, 1, NONE, fonts());
    assert!(!e.editing_wrap_points() && e.view(0..1).wrap_points.is_none());
}

fn protect(e: &mut Editor) {
    let p = crate::api::command::DocumentProtection {
        kind: Some(crate::api::command::ProtectionKind::ReadOnly),
        enforce: Some(true),
        ..Default::default()
    };
    assert!(matches!(e.execute(Command::SetDocumentProtection(p), fonts()), Outcome::Changed(_)));
}

#[test]
fn wrap_points_are_kept_by_document_protection() {
    let mut e = two_shapes();
    press(&mut e, 150.0, 350.0, NONE);
    protect(&mut e);
    assert!(matches!(e.edit_wrap_points(), Outcome::Refused(_)), "not entered");
    let mut e = two_shapes();
    press(&mut e, 150.0, 350.0, NONE);
    assert!(matches!(e.edit_wrap_points(), Outcome::Moved));
    let [x, y] = wrap_points(&mut e)[0];
    protect(&mut e);
    let depth = e.history.undo_depth();
    drag_doc(&mut e, [x, y], [x + 20.0, y + 20.0], NONE);
    assert_eq!((poly(&e), e.history.undo_depth()), (None, depth), "the polygon stays");
}

#[test]
fn a_point_drag_ends_without_a_change_when_another_object_is_selected() {
    let mut e = two_shapes();
    press(&mut e, 150.0, 350.0, NONE);
    assert!(matches!(e.edit_wrap_points(), Outcome::Moved));
    let [x, y] = wrap_points(&mut e)[0];
    let depth = e.history.undo_depth();
    e.pointer_input(PointerKind::Down, x, y, 1, NONE, fonts());
    e.pointer_input(PointerKind::Move, x + 20.0, y + 20.0, 0, NONE, fonts());
    let other = FloatId(all_floats(&e)[1]);
    e.selection = Selection::Object(ObjectRef::Floating(other));
    e.pointer_input(PointerKind::Up, x + 20.0, y + 20.0, 0, NONE, fonts());
    assert_eq!((poly(&e), e.history.undo_depth()), (None, depth), "nothing committed");
    assert_eq!(e.selection(), Selection::Object(ObjectRef::Floating(other)), "the selection stays");
}

#[test]
fn escape_ends_wrap_points_and_an_inline_picture_has_none() {
    let mut e = two_shapes();
    press(&mut e, 150.0, 350.0, NONE);
    assert!(matches!(e.edit_wrap_points(), Outcome::Moved));
    e.execute(Command::Escape, fonts());
    assert!(!e.editing_wrap_points());
    assert!(matches!(e.selection(), Selection::Object(_)), "the object stays selected");
    let mut e = with_inserted_picture();
    assert!(matches!(e.edit_wrap_points(), Outcome::Refused(_)), "in line with the text");
}

#[test]
fn regroup_groups_again_what_ungroup_made_until_one_is_gone() {
    let mut e = two_shapes();
    select_both(&mut e);
    assert!(changed(&e.group_objects(fonts())));
    assert!(e.regroupable().is_none(), "nothing ungrouped yet");
    assert!(changed(&e.ungroup_objects(fonts())));
    let made = e.regroupable().map(<[FloatId]>::to_vec).expect("the members");
    assert_eq!(made.len(), 2);
    press(&mut e, 400.0, 600.0, NONE);
    let depth = e.history.undo_depth();
    let out = e.regroup_objects(fonts());
    assert!(changed(&out), "{out:?}");
    assert_eq!((all_floats(&e).len(), e.history.undo_depth()), (1, depth + 1), "one group, one step");
    assert!(e.regroupable().is_none());
    assert!(changed(&e.ungroup_objects(fonts())));
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(e.regroupable().is_none(), "an undone ungroup leaves nothing to regroup");
    assert!(matches!(e.regroup_objects(fonts()), Outcome::Refused(_)));
}

#[test]
fn the_default_picture_wrap_inserts_in_line_in_a_note() {
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut e = Editor::open(&super::blank_with("Body text here"), fonts()).expect("opens");
    e.set_default_picture_wrap(ObjectWrap::Square);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 0), Unit(2))));
    e.execute(Command::InsertFootnote, fonts());
    let Some(StoryId::Footnote(note)) = e.caret().map(|p| p.story) else { panic!("the caret is in the note") };
    let out = e.execute(Command::InsertPicture(Picture { data: Bytes(data), alt_text: None }), fonts());
    assert!(changed(&out), "{out:?}");
    let inlines: Vec<&Inline> =
        e.document().footnotes[&note].iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).flat_map(|p| &p.inlines).collect();
    assert_eq!(inlines.iter().filter(|i| matches!(i, Inline::Picture(..))).count(), 1, "the picture is in line in the note");
    assert!(!inlines.iter().any(|i| matches!(i, Inline::Float(..))), "a note holds no floating object");
    assert!(all_floats(&e).is_empty());
}

#[test]
fn the_default_picture_wrap_floats_an_inserted_picture_in_one_step() {
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut e = Editor::open(&super::blank_with("Body text here"), fonts()).expect("opens");
    assert_eq!(e.default_picture_wrap(), ObjectWrap::Inline);
    e.set_default_picture_wrap(ObjectWrap::Square);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, super::para_id(&e, 0), Unit(2))));
    let depth = e.history.undo_depth();
    let out = e.execute(Command::InsertPicture(Picture { data: Bytes(data), alt_text: Some("A".into()) }), fonts());
    assert!(changed(&out), "{out:?}");
    assert_eq!(e.history.undo_depth(), depth + 1, "one step");
    let f = all_floats(&e);
    assert_eq!(f.len(), 1, "floating");
    let fl = e.float_of(FloatId(f[0])).expect("the float");
    assert_eq!((fl.wrap, &*fl.alt), (crate::docx::FloatWrap::Square, "A"));
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(all_floats(&e).is_empty());
}

fn saved_xml(e: &Editor) -> String {
    let saved = crate::docx::write::save(e.document()).expect("saves");
    crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part")
}

#[test]
fn an_inline_picture_takes_shape_formatting_and_saves_it() {
    use crate::api::drawing::{ColourValue, EffectsChange, Glow};
    let mut e = with_inserted_picture();
    let glow = Glow { colour: ColourValue::rgb(crate::api::value::Color(0xFF0000)), radius: Points(6.0) };
    let out = e.execute(Command::SetShapeEffects(EffectsChange { glow: Some(Some(glow)), ..Default::default() }), fonts());
    assert!(changed(&out), "{out:?}");
    assert!(
        matches!(e.object_shape().map(|s| &s.kind), Some(crate::drawing::ShapeKind::Pic(p)) if p.effects.as_ref().is_some_and(|x| x.glow.is_some()))
    );
    assert!(changed(&e.execute(Command::FlipObject(crate::api::drawing::FlipAxis::Horizontal), fonts())));
    let xml = saved_xml(&e);
    assert!(xml.contains("<a:glow") && xml.contains(r#"flipH="1""#), "{xml}");
    e.undo(true, fonts());
    e.undo(true, fonts());
    assert!(e.object_shape().is_some_and(|s| !s.xfrm.flip_h), "two steps");
}

#[test]
fn a_turned_inline_picture_takes_its_turned_bounds_in_the_line() {
    let mut e = inline_picture(200, 100, "", "", "rect", "");
    let (w, h) = first_inline_picture(&e).map(|p| (p.w, p.h)).expect("the picture");
    assert!((w - h).abs() > 50.0, "not square: {w} x {h}");
    let line = |e: &mut Editor| {
        e.complete(fonts());
        let g = e.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
        let l = g.lines.first().expect("the picture's line");
        l.bottom - l.top
    };
    let tall = line(&mut e);
    let out = e.execute(Command::SetObjectRotation(90.0), fonts());
    assert!(changed(&out), "{out:?}");
    let now = line(&mut e);
    assert!((now - tall - (w - h)).abs() < 1.0, "the line as tall as the turned picture: {tall} → {now}");
    let p = first_inline_picture(&e).expect("the picture");
    let (dx, dy) = ((h - w) / 2.0, (w - h) / 2.0);
    assert!((p.effect[0] - dx).abs() < 0.01 && (p.effect[1] - dy).abs() < 0.01, "{:?} for {w} x {h}", p.effect);
    assert_eq!(p.shape.as_ref().map(|s| s.xfrm.rot.0), Some(90 * 60000));
    let xml = saved_xml(&e);
    let ee = format!(r#"effectExtent l="{}" t="{}""#, (dx * 12700.0).round() as i64, (dy * 12700.0).round() as i64);
    assert!(xml.contains(&ee) && xml.contains(r#"rot="5400000""#), "{ee} in {xml}");
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(first_inline_picture(&again).map(|p| p.effect), first_inline_picture(&e).map(|p| p.effect));
    // Resizing keeps the turned bounds.
    let out = e.execute(Command::SetObjectSize(ObjectSize { width: Some(Points(w / 2.0)), lock_aspect: Some(true), ..Default::default() }), fonts());
    assert!(changed(&out), "{out:?}");
    let p = first_inline_picture(&e).expect("the picture");
    assert!((p.effect[0] - (p.h - p.w) / 2.0).abs() < 0.01, "{:?} for {} x {}", p.effect, p.w, p.h);
}

#[test]
fn an_inline_picture_turns_by_its_rotate_knob() {
    let mut e = with_inserted_picture();
    e.complete(fonts());
    let frames = e.shape_frames(0..1);
    let [f] = &frames[..] else { panic!("one frame: {frames:?}") };
    let [x0, y0, x1, y1] = f.rect;
    let c = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
    // From above the centre round to its right: a quarter turn.
    let to = [c[0] + (c[1] - f.rotate[1]), c[1]];
    drag(&mut e, f.rotate, to, NONE);
    let p = first_inline_picture(&e).expect("the picture");
    assert_eq!(p.shape.as_ref().map(|s| s.xfrm.rot.0), Some(90 * 60000), "{:?}", e.selection());
    assert!(p.effect[0] != 0.0, "the line box turns too: {:?}", p.effect);
    e.undo(true, fonts());
    assert_eq!(first_inline_picture(&e).and_then(|p| p.shape.as_ref()).map(|s| s.xfrm.rot.0), Some(0));
}

#[test]
fn an_inline_shape_shows_its_adjust_handles_and_takes_their_drags() {
    use crate::api::drawing::ShapeInsert;
    let mut e = Editor::open(&bytes("<w:p><w:r><w:t>Text</w:t></w:r></w:p>").expect("a package"), fonts()).expect("opens");
    let out =
        e.execute(Command::InsertShape(ShapeInsert { preset: ShapePreset::from_name("roundRect").unwrap(), width: None, height: None }), fonts());
    assert!(changed(&out), "{out:?}");
    assert!(changed(&e.execute(Command::SetObjectWrap(ObjectWrap::Inline), fonts())));
    assert!(matches!(e.selection(), Selection::Object(ObjectRef::Inline { .. })), "{:?}", e.selection());
    e.complete(fonts());
    let frames = e.shape_frames(0..1);
    let [f] = &frames[..] else { panic!("one frame: {frames:?}") };
    let [h] = f.adjust[..] else { panic!("one adjust handle: {:?}", f.adjust) };
    let shape = e.selection();
    let before = e.object_shape().and_then(|s| s.sp()).map(|s| s.geom.adjust_values()).expect("its adjustments");
    drag(&mut e, h, [h[0] + 10.0, h[1]], NONE);
    assert_eq!(e.selection(), shape, "the same shape stays selected");
    let after = e.object_shape().and_then(|s| s.sp()).map(|s| s.geom.adjust_values()).expect("its adjustments");
    assert_ne!(before, after);
}

#[test]
fn the_selection_pane_lists_renames_hides_and_restacks() {
    let mut e = two_shapes();
    let ids = all_floats(&e);
    let (star, ellipse) = (FloatId(ids[0]), FloatId(ids[1]));
    let list = e.object_list(0, fonts());
    let names: Vec<_> = list.iter().map(|o| (o.id, o.name.as_str(), o.hidden)).collect();
    assert_eq!(names, vec![(ellipse, "S2", false), (star, "S1", false)], "front first");
    // Rename and title the star.
    assert!(e.select_listed(star, &[], false) == Outcome::Moved);
    assert!(changed(&e.set_object_name("Gold star", fonts())));
    assert!(changed(&e.set_object_title("A star", fonts())));
    let props = e.object_props(fonts()).expect("props");
    assert_eq!((props.name.as_str(), props.title.as_str(), props.stack_index), ("Gold star", "A star", Some(1)));
    // Bring it to the front, in front of the ellipse.
    assert!(changed(&e.restack_object(star, Some(ellipse), fonts())));
    assert_eq!(e.object_list(0, fonts()).iter().map(|o| o.id).collect::<Vec<_>>(), vec![star, ellipse]);
    assert!(changed(&e.restack_object(star, None, fonts())));
    assert_eq!(e.object_list(0, fonts()).iter().map(|o| o.id).collect::<Vec<_>>(), vec![ellipse, star]);
    // Hide the ellipse: listed still, not drawn.
    let shapes_drawn = |e: &mut Editor| drawn(e).len();
    assert_eq!(shapes_drawn(&mut e), 2);
    assert!(changed(&e.set_object_hidden(ellipse, &[], true, fonts())));
    assert_eq!(shapes_drawn(&mut e), 1);
    assert_eq!(e.object_list(0, fonts()).iter().map(|o| (o.id, o.hidden)).collect::<Vec<_>>(), vec![(ellipse, true), (star, false)]);
    // Saved and read again.
    let xml = saved_xml(&e);
    assert!(xml.contains(r#"name="Gold star""#) && xml.contains(r#"title="A star""#) && xml.contains(r#"hidden="1""#), "{xml}");
    let mut again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("opens");
    again.complete(fonts());
    let back: Vec<_> = again.object_list(0, fonts()).into_iter().map(|o| (o.name, o.hidden)).collect();
    assert_eq!(back, vec![("S2".to_string(), true), ("Gold star".to_string(), false)]);
    // Shown again.
    assert!(changed(&e.set_object_hidden(ellipse, &[], false, fonts())));
    assert_eq!(shapes_drawn(&mut e), 2);
}

#[test]
fn restacking_takes_equal_z_floats_as_they_are_drawn() {
    let shape = |c: &str| sp(0, 0, 60, 60, "rect", "", c);
    let mut e = editor(&format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}{}{}</w:p>",
        anchor(100, 300, 60, 60, 1, false, 5, &shape("FF0000")),
        anchor(200, 300, 60, 60, 2, false, 5, &shape("00FF00")),
        anchor(300, 300, 60, 60, 3, false, 1, &shape("0000FF"))
    ));
    let ids = all_floats(&e).into_iter().map(FloatId).collect::<Vec<_>>();
    let [a, b, x] = ids[..] else { panic!("three floats: {ids:?}") };
    // The first one drawn last among equal z: in front of the second.
    let Block::Para(p) = &mut e.doc.sections[0].blocks[0] else { panic!("a paragraph") };
    for i in &mut p.inlines {
        if let Inline::Float(f, _) = i
            && FloatId(f.id) == a
        {
            f.order = 99;
        }
    }
    let front_first = |e: &mut Editor| e.object_list(0, fonts()).iter().map(|o| o.id).collect::<Vec<_>>();
    assert_eq!(front_first(&mut e), [a, b, x]);
    assert!(changed(&e.restack_object(x, Some(b), fonts())));
    assert_eq!(front_first(&mut e), [a, x, b], "just in front of the one behind, still behind the other");
}

#[test]
fn a_group_child_is_listed_renamed_and_hidden_in_its_group() {
    let mut e = two_shapes();
    select_both(&mut e);
    assert!(changed(&e.group_objects(fonts())));
    let list = e.object_list(0, fonts());
    let [g] = &list[..] else { panic!("one group: {list:?}") };
    assert_eq!((g.kind, g.children.len()), (crate::api::state::ObjectKind::Group, 2));
    let child = g.children[0].path.clone();
    assert!(e.select_listed(g.id, &child, false) == Outcome::Moved);
    assert!(changed(&e.set_object_name("Inner", fonts())));
    assert!(changed(&e.set_object_hidden(g.id, &child, true, fonts())));
    assert_eq!(drawn(&mut e).len(), 1);
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("opens");
    let tree = again.float_of(g.id).or_else(|| again.float_of(FloatId(all_floats(&again)[0]))).and_then(|f| f.shape.clone()).expect("the group");
    let c = tree.at(&child).expect("the child");
    assert_eq!((c.nv.name.as_str(), c.nv.hidden), ("Inner", true));
    // The group selected: hiding a child keeps it; hiding the group leaves it.
    let other = g.children[1].path.clone();
    assert!(e.select_listed(g.id, &[], false) == Outcome::Moved);
    assert!(changed(&e.set_object_hidden(g.id, &other, true, fonts())));
    assert!(matches!(e.selection(), Selection::Object(_)), "the group stays selected");
    assert!(changed(&e.set_object_hidden(g.id, &[], true, fonts())));
    assert!(!matches!(e.selection(), Selection::Object(_)), "the hidden group is left");
}

#[test]
fn a_new_pictures_name_and_title_are_saved() {
    let mut e = with_inserted_picture();
    assert!(changed(&e.set_object_name("Logo", fonts())));
    assert!(changed(&e.set_object_title("Company logo", fonts())));
    let xml = saved_xml(&e);
    assert!(xml.contains(r#"name="Logo""#) && xml.contains(r#"title="Company logo""#), "{xml}");
    assert!(changed(&e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts())));
    let xml = saved_xml(&e);
    assert!(xml.contains(r#"name="Logo""#) && xml.contains(r#"title="Company logo""#), "floating: {xml}");
}

#[test]
fn a_tight_wrap_takes_the_shapes_outline_until_its_points_are_edited() {
    let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(100, 300, 100, 100, 1, false, 5, &star)));
    let f = all_floats(&e)[0];
    select(&mut e, f);
    assert!(changed(&e.execute(Command::SetObjectWrap(ObjectWrap::Tight), fonts())));
    let made = e.selected_wrap_polygon().expect("a polygon");
    assert!(!made.edited && made.points.len() == 10, "a star's ten corners: {made:?}");
    let xml = saved_xml(&e);
    assert!(xml.contains(r#"wrapPolygon edited="0""#), "{xml}");
    // Edited by hand: kept through a change of shape.
    let square = [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)];
    assert!(changed(&e.set_wrap_points(&square, fonts())));
    assert!(changed(&e.execute(Command::SetShapePreset(ShapePreset::from_name("ellipse").unwrap()), fonts())));
    let kept = e.selected_wrap_polygon().expect("a polygon");
    assert_eq!((kept.points.len(), kept.edited), (4, true));
    assert!(saved_xml(&e).contains(r#"wrapPolygon edited="1""#));
    // Reset: the ellipse's outline.
    assert!(changed(&e.reset_wrap_points(fonts())));
    let reset = e.selected_wrap_polygon().expect("a polygon");
    assert!(!reset.edited && reset.points.len() > 16, "{reset:?}");
    assert!(reset.points.iter().all(|&(x, y)| ((x - 0.5).powi(2) + (y - 0.5).powi(2) - 0.25).abs() < 0.01), "on the ellipse");
    // Hostile points are refused.
    assert!(!changed(&e.set_wrap_points(&[(0.0, 0.0), (f64::NAN, 1.0), (1.0, 1.0)], fonts())));
    assert!(!changed(&e.set_wrap_points(&[(0.0, 0.0); 2], fonts())));
}

#[test]
fn a_wrap_polygon_of_up_to_1024_points_is_saved_and_read_whole() {
    for n in [1023usize, 1024] {
        let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
        let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(100, 300, 100, 100, 1, false, 5, &star)));
        let f = all_floats(&e)[0];
        select(&mut e, f);
        // Round a circle: every vertex its own
        let pts: Vec<(f64, f64)> = (0..n)
            .map(|k| {
                let a = k as f64 / n as f64 * std::f64::consts::TAU;
                (0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin())
            })
            .collect();
        assert!(changed(&e.set_wrap_points(&pts, fonts())), "{n}");
        let mut again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
        again.complete(fonts());
        let g = FloatId(all_floats(&again)[0]);
        again.set_selection(Selection::Object(ObjectRef::Floating(g)));
        let read = again.selected_wrap_polygon().expect("a polygon");
        assert_eq!(read.points.len(), n, "every vertex kept");
    }
}

#[test]
fn replacing_the_wrap_polygon_ends_a_point_drag() {
    let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(100, 300, 100, 100, 1, false, 5, &star)));
    let f = all_floats(&e)[0];
    select(&mut e, f);
    assert!(changed(&e.set_wrap_points(&[(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)], fonts())));
    assert!(e.edit_wrap_points() == Outcome::Moved);
    let w = e.wrap_points(0..1).expect("the points");
    let v = w.points[0];
    let (x, y) = e.stack().to_doc(w.page, v[0], v[1]).expect("on the stack");
    e.pointer_input(PointerKind::Down, x, y, 1, NONE, fonts());
    e.pointer_input(PointerKind::Move, x + 30.0, y + 30.0, 0, NONE, fonts());
    assert!(changed(&e.reset_wrap_points(fonts())));
    let reset = e.selected_wrap_polygon().expect("a polygon");
    e.pointer_input(PointerKind::Up, x + 30.0, y + 30.0, 0, NONE, fonts());
    assert_eq!(e.selected_wrap_polygon(), Some(reset), "the drag of the old polygon isn't applied to the new");
}

#[test]
fn select_objects_takes_the_floats_wholly_inside_its_rectangle() {
    let mut e = two_shapes();
    let ids = all_floats(&e);
    e.set_select_objects(true);
    // Round the star (100..200, 300..400) only.
    drag(&mut e, [90.0, 290.0], [210.0, 410.0], NONE);
    assert_eq!(e.selected_floats(), vec![FloatId(ids[0])]);
    // Round both.
    drag(&mut e, [90.0, 290.0], [370.0, 410.0], NONE);
    let mut got = e.selected_floats();
    got.sort();
    assert_eq!(got, vec![FloatId(ids[0]), FloatId(ids[1])]);
    assert!(e.marquee_rect().is_none(), "gone once let go");
    // Escape ends it; a drag then selects text again.
    let _ = e.execute(Command::Escape, fonts());
    assert!(!e.selecting_objects());
}

#[test]
fn the_anchor_drags_to_another_paragraph_and_the_object_stays() {
    let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
    let float =
        anchor(300, 300, 100, 100, 1, false, 5, &star).replace(r#"<wp:positionV relativeFrom="page">"#, r#"<wp:positionV relativeFrom="paragraph">"#);
    let paras: String =
        (0..12).map(|k| format!("<w:p><w:r><w:t>Paragraph {k}</w:t></w:r>{}</w:p>", if k == 0 { float.as_str() } else { "" })).collect();
    let mut e = editor(&paras);
    let f = FloatId(all_floats(&e)[0]);
    select(&mut e, f.0);
    let before = drawn(&mut e);
    let v = e.view(0..1);
    let (_, [ax, ay]) = v.anchor.expect("the anchor shows");
    let u = 0.75 / e.viewport.zoom.max(0.1);
    let ring = [(ax - 18.0 * u).max(10.0 * u), ay + 8.0 * u];
    let target = body_para(&e, 6);
    let geo = e.pages.page(0, &e.index).expect("page 0");
    let top = geo.lines.iter().find(|l| l.shows(target)).expect("its line").top;
    drag(&mut e, ring, [ring[0], top + 2.0], NONE);
    assert_eq!(e.index().float_anchor(f), Some(target), "anchored in the seventh paragraph");
    assert_eq!(drawn(&mut e), before, "drawn where it was");
    e.undo(true, fonts());
    assert_ne!(e.index().float_anchor(f), Some(target));
}

fn brighter(e: &mut Editor) {
    use crate::drawing::paint::BlipAdjust;
    use crate::drawing::units::Pct;
    assert!(changed(&e.set_picture_adjust(&[BlipAdjust::Lum { bright: Pct(30_000), contrast: Pct(0) }], fonts())));
}

#[test]
fn reset_picture_and_size_drops_a_relative_size() {
    for and_size in [false, true] {
        let (mut e, f) = float_picture();
        for s in &mut e.doc.sections {
            for b in &mut s.blocks {
                let Block::Para(p) = b else { continue };
                for i in &mut p.inlines {
                    if let Inline::Float(fl, _) = i
                        && FloatId(fl.id) == f
                    {
                        fl.relative.size = [Some(("page".into(), 500)), None];
                    }
                }
            }
        }
        brighter(&mut e);
        assert!(changed(&e.reset_picture(and_size, fonts())));
        let size = e.float_of(f).map(|n| n.relative.size.clone()).expect("float");
        assert_eq!(size[0].is_none(), and_size, "and_size {and_size}: {size:?}");
    }
}

#[test]
fn reset_picture_undoes_a_quarter_turned_floats_turn_and_box() {
    for and_size in [false, true] {
        let (mut e, f) = float_picture();
        let was = e.float_of(f).cloned().expect("float");
        assert!(changed(&e.execute(Command::SetObjectRotation(90.0), fonts())));
        brighter(&mut e);
        let turned = e.float_of(f).cloned().expect("float");
        assert!(turned.rot_swap && (turned.w - was.h).abs() < 0.1, "a quarter turn swaps the box");
        let depth = e.history.undo_depth();
        assert!(changed(&e.reset_picture(and_size, fonts())), "{and_size}");
        assert_eq!(e.history.undo_depth(), depth + 1, "one step");
        let n = e.float_of(f).cloned().expect("float");
        assert!(!n.rot_swap, "the box isn't turned");
        assert_eq!(n.shape.as_ref().map(|s| s.xfrm.rot.0), Some(0));
        assert_eq!(e.picture_adjust().map(|a| a.brightness), Some(0.0));
        let (w, h) = if and_size { natural(&e, &n) } else { (was.w, was.h) };
        assert!((n.w - w).abs() < 0.1 && (n.h - h).abs() < 0.1, "({}, {}) vs ({w}, {h}) and_size {and_size}", n.w, n.h);
        let xml = saved_xml(&e);
        let extent = format!(r#"<wp:extent cx="{}" cy="{}""#, (n.w * 12700.0).round() as i64, (n.h * 12700.0).round() as i64);
        assert!(xml.contains(&extent), "{extent}");
        e.undo(true, fonts());
        let back = e.float_of(f).cloned().expect("float");
        assert!(back.rot_swap && (back.w - turned.w).abs() < 1e-9 && (back.h - turned.h).abs() < 1e-9);
        assert_eq!(e.picture_adjust().map(|a| a.brightness), Some(0.3));
    }
}

#[test]
fn reset_picture_on_a_turned_inline_picture_drops_its_turned_bounds() {
    for and_size in [false, true] {
        let mut e = with_inserted_picture();
        let (w, h) = first_inline_picture(&e).map(|p| (p.w, p.h)).expect("the picture");
        assert!(changed(
            &e.execute(Command::SetObjectSize(ObjectSize { width: Some(Points(w / 2.0)), lock_aspect: Some(false), ..Default::default() }), fonts())
        ));
        assert!(changed(&e.execute(Command::SetObjectRotation(90.0), fonts())));
        let turned = first_inline_picture(&e).map(|p| p.effect).expect("the picture");
        assert!(turned[0].abs() > 0.01, "{turned:?}");
        assert!(changed(&e.reset_picture(and_size, fonts())));
        let p = first_inline_picture(&e).expect("the picture");
        assert!(p.effect.iter().all(|v| v.abs() < 0.01), "{:?} and_size {and_size}", p.effect);
        assert_eq!(p.shape.as_ref().map(|s| s.xfrm.rot.0), Some(0));
        let want = if and_size { (w, h) } else { (w / 2.0, h) };
        assert!((p.w - want.0).abs() < 0.1 && (p.h - want.1).abs() < 0.1, "({}, {}) vs {want:?}", p.w, p.h);
        e.undo(true, fonts());
        assert_eq!(first_inline_picture(&e).map(|p| p.effect), Some(turned), "one undo");
    }
}

#[test]
fn picture_recolor_reads_back_saves_and_replaces_the_one_before() {
    use crate::drawing::colour::Colour;
    use crate::editing::pictures::Recolor;
    let mut e = with_inserted_picture();
    brighter(&mut e);
    let a = e.picture_adjust().expect("a picture");
    assert_eq!((a.brightness, a.contrast, a.transparency, a.recolor), (0.3, 0.0, 0.0, Recolor::None));
    let cases = [
        (Recolor::Grayscale, "<a:grayscl/>"),
        (Recolor::BlackAndWhite(0.5), r#"<a:biLevel thresh="50000"/>"#),
        (Recolor::Duotone(Colour::srgb(0x000000), Colour::srgb(0xFFCC00)), "<a:duotone>"),
    ];
    for (recolor, xml) in cases {
        let depth = e.history.undo_depth();
        assert!(changed(&e.set_picture_recolor(recolor.clone(), fonts())), "{recolor:?}");
        assert_eq!(e.history.undo_depth(), depth + 1, "one step");
        let a = e.picture_adjust().expect("a picture");
        assert_eq!((a.brightness, &a.recolor), (0.3, &recolor), "the brightness stays");
        let saved = saved_xml(&e);
        assert!(saved.contains(xml), "{xml} in {saved}");
        let others = ["<a:grayscl/>", "<a:biLevel", "<a:duotone>"].into_iter().filter(|x| saved.contains(x)).count();
        assert_eq!(others, 1, "one recolouring: {saved}");
        let mut again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
        select_first_inline_picture(&mut again);
        assert_eq!(again.picture_adjust().map(|a| a.recolor), Some(recolor));
    }
    assert!(!changed(&e.set_picture_recolor(Recolor::BlackAndWhite(1.5), fonts())), "a threshold is 0 to 1");
    e.undo(true, fonts());
    assert!(matches!(e.picture_adjust().map(|a| a.recolor), Some(Recolor::BlackAndWhite(t)) if t == 0.5), "undone to the one before");
    assert!(changed(&e.set_picture_recolor(Recolor::None, fonts())));
    assert_eq!(e.picture_adjust().map(|a| a.recolor), Some(Recolor::None));
}

#[test]
fn a_pictures_image_and_a_shapes_picture_fill_read_back() {
    use crate::drawing::paint::{BlipFill, BlipMode, Fill};
    use crate::drawing::units::Pct;
    let data = open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let mut e = with_inserted_picture();
    assert_eq!(e.object_image(), Some(&data[..]));
    assert_eq!(e.shape_fill_image(), None, "a picture has no fill");
    let k = e.document().images.iter().position(|i| *i == data).expect("the image");
    let _ = e.execute(Command::Escape, fonts());
    assert!(changed(
        &e.execute(Command::InsertShape(ShapeInsert { preset: ShapePreset::from_name("rect").unwrap(), width: None, height: None }), fonts())
    ));
    assert_eq!(e.object_image(), None, "a shape has no picture");
    let fill =
        Fill::Blip(BlipFill { image: Some(k), src: [Pct(0); 4], mode: BlipMode::Stretch([Pct(0); 4]), adjust: Vec::new(), rot_with_shape: true });
    assert!(changed(&e.set_shape_fill(Some(fill), fonts())));
    assert_eq!(e.shape_fill_image(), Some(&data[..]));
}

#[test]
fn lock_aspect_off_on_an_inline_picture_is_saved() {
    let mut e = with_inserted_picture();
    assert!(e.object_props(fonts()).is_some_and(|p| p.lock_aspect));
    let depth = e.history.undo_depth();
    assert!(changed(&e.set_lock_aspect(false, fonts())));
    assert_eq!(e.history.undo_depth(), depth + 1);
    assert!(e.object_props(fonts()).is_some_and(|p| !p.lock_aspect));
    let xml = saved_xml(&e);
    assert!(!xml.contains(r#"noChangeAspect="1""#), "{xml}");
    let mut again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    select_first_inline_picture(&mut again);
    assert!(again.object_props(fonts()).is_some_and(|p| !p.lock_aspect), "read back off");
    e.undo(true, fonts());
    assert!(e.object_props(fonts()).is_some_and(|p| p.lock_aspect));
    assert!(saved_xml(&e).contains(r#"noChangeAspect="1""#), "on again");
}

#[test]
fn a_read_pictures_title_and_lock_are_saved_without_a_resize() {
    let saved = crate::docx::write::save(with_inserted_picture().document()).expect("saves");
    let reopen = || {
        let mut e = Editor::open(&saved, fonts()).expect("reopens");
        select_first_inline_picture(&mut e);
        e
    };
    let mut e = reopen();
    assert!(changed(&e.set_object_title("A title", fonts())));
    assert!(saved_xml(&e).contains(r#"title="A title""#), "{}", saved_xml(&e));
    let mut e = reopen();
    assert!(saved_xml(&e).contains(r#"noChangeAspect="1""#));
    assert!(changed(&e.set_lock_aspect(false, fonts())));
    assert!(!saved_xml(&e).contains(r#"noChangeAspect="1""#), "{}", saved_xml(&e));
}

#[test]
fn restack_and_hide_undo_and_redo_one_step_each() {
    let mut e = two_shapes();
    let ids = all_floats(&e);
    let (star, ellipse) = (FloatId(ids[0]), FloatId(ids[1]));
    let order = |e: &mut Editor| e.object_list(0, fonts()).iter().map(|o| (o.id, o.hidden)).collect::<Vec<_>>();
    let first = order(&mut e);
    assert!(changed(&e.restack_object(star, Some(ellipse), fonts())));
    let restacked = order(&mut e);
    assert!(changed(&e.set_object_hidden(ellipse, &[], true, fonts())));
    let hidden = order(&mut e);
    e.undo(true, fonts());
    assert_eq!(order(&mut e), restacked);
    e.undo(true, fonts());
    assert_eq!(order(&mut e), first);
    let _ = e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(order(&mut e), restacked);
    let _ = e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(order(&mut e), hidden);
}

#[test]
fn selection_pane_edits_need_the_object_editable() {
    use crate::api::command::{DocumentProtection, ProtectionKind};
    let star = anchor(100, 300, 100, 100, 1, false, 5, &sp(0, 0, 100, 100, "star5", "", "FF0000"));
    let ellipse = anchor(260, 340, 100, 60, 2, false, 6, &sp(0, 0, 100, 60, "ellipse", "", "00FF00"));
    let body = format!(
        r#"<w:p><w:permStart w:id="1" w:edGrp="everyone"/><w:r><w:t>In</w:t></w:r>{star}<w:permEnd w:id="1"/></w:p><w:p><w:r><w:t>Out</w:t></w:r>{ellipse}</w:p>"#
    );
    let mut e = editor(&body);
    let read_only = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    assert!(changed(&e.execute(Command::SetDocumentProtection(read_only), fonts())));
    let ids = all_floats(&e);
    let (open_one, shut) = (FloatId(ids[0]), FloatId(ids[1]));
    select(&mut e, open_one.0);
    assert!(!changed(&e.set_object_hidden(shut, &[], true, fonts())), "a protected object stays shown");
    assert!(!changed(&e.restack_object(shut, None, fonts())), "or in its place");
    assert!(!changed(&e.restack_object(open_one, Some(shut), fonts())), "in front of a protected one moves it");
    // Object commands stay refused in an editable range too, as the selection's object commands are.
    assert!(!changed(&e.set_object_hidden(open_one, &[], true, fonts())));
    // A protected object's anchor doesn't drag.
    select(&mut e, shut.0);
    let v = e.view(0..1);
    let (_, [ax, ay]) = v.anchor.expect("the anchor shows");
    let u = 0.75 / e.viewport.zoom.max(0.1);
    let ring = [(ax - 18.0 * u).max(10.0 * u), ay + 8.0 * u];
    assert!(e.anchor_hit(ring[0], ring[1]).is_none(), "a protected object's anchor stays");
}

#[test]
fn a_hostile_wrap_polygon_is_thinned_on_read() {
    let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
    let n = 200_000;
    let mut poly = String::from(r#"<wp:wrapPolygon edited="1"><wp:start x="0" y="0"/>"#);
    for k in 1..n {
        poly.push_str(&format!(r#"<wp:lineTo x="{}" y="{}"/>"#, k % 21600, (k * 7) % 21600));
    }
    poly.push_str(r#"<wp:lineTo x="0" y="0"/></wp:wrapPolygon>"#);
    let float = anchor(100, 300, 100, 100, 1, false, 5, &star)
        .replace("<wp:wrapNone/>", &format!(r#"<wp:wrapTight wrapText="bothSides">{poly}</wp:wrapTight>"#));
    assert!(float.contains("wrapTight"), "the anchor helper writes wrapNone");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{float}</w:p>"));
    let f = all_floats(&e)[0];
    let p = e.float_of(FloatId(f)).and_then(|fl| fl.poly.clone()).expect("a polygon");
    assert!(p.len() <= crate::docx::MAX_WRAP_POINTS && p.len() > 3, "{}", p.len());
    assert_eq!(p.first(), p.last(), "still closed");
    select(&mut e, f);
    assert!(e.selected_wrap_polygon().is_some_and(|w| w.points.len() < crate::docx::MAX_WRAP_POINTS));
}

/// A one-section document whose default header's paragraph holds `header` and whose body is `body`.
fn with_header(body: &str, header: &str) -> Editor {
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#;
    let hdr = format!(r#"<?xml version="1.0"?><w:hdr {ns}><w:p>{header}</w:p></w:hdr>"#);
    let document = format!(
        r#"<?xml version="1.0"?><w:document {ns}><w:body>{body}<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let b = crate::layout::tests::package_bytes(&[
        ("[Content_Types].xml", ct),
        ("_rels/.rels", rels),
        ("word/_rels/document.xml.rels", doc_rels),
        ("word/header1.xml", &hdr),
        ("word/document.xml", &document),
    ])
    .expect("a package");
    let mut e = Editor::open(&b, fonts()).expect("opens");
    e.complete(fonts());
    e
}

#[test]
fn restacking_body_objects_leaves_a_header_object_between_them() {
    let a = anchor(100, 300, 100, 100, 1, false, 1, &sp(0, 0, 100, 100, "rect", "", "FF0000"));
    let b = anchor(260, 340, 100, 60, 3, false, 3, &sp(0, 0, 100, 60, "ellipse", "", "00FF00"));
    let logo = anchor(72, 36, 40, 20, 2, false, 2, &sp(0, 0, 40, 20, "rect", "", "0000FF"));
    let mut e = with_header(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{a}{b}</w:p>"), &format!("<w:r><w:t>Head</w:t></w:r>{logo}"));
    let ids = all_floats(&e);
    let (a, b) = (FloatId(ids[0]), FloatId(ids[1]));
    let header = e.stacking().into_iter().map(|(_, g)| g).find(|g| !ids.contains(&g.0)).expect("the header's float");
    assert!(changed(&e.restack_object(b, Some(a), fonts())), "in front of the other body object");
    assert_eq!(e.float_of(header).map(|f| f.z), Some(2), "the header's float stays");
    assert_eq!(e.object_list(0, fonts()).iter().map(|o| o.id).collect::<Vec<_>>(), vec![b, a]);
    assert!(changed(&e.restack_object(a, Some(b), fonts())));
    assert_eq!(e.object_list(0, fonts()).iter().map(|o| o.id).collect::<Vec<_>>(), vec![a, b]);
    assert_eq!(e.float_of(header).map(|f| f.z), Some(2));
    assert!(!changed(&e.restack_object(a, Some(header), fonts())), "not in front of a header's float");
}

#[test]
fn objects_with_equal_stacking_list_in_drawn_order() {
    let r = |x, id| anchor(x, 300, 60, 60, id, false, 4, &sp(0, 0, 60, 60, "rect", "", "FF0000"));
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}{}{}</w:p>", r(100, 1), r(200, 2), r(300, 3)));
    let ids: Vec<FloatId> = all_floats(&e).into_iter().map(FloatId).collect();
    let list = |e: &mut Editor| e.object_list(0, fonts()).iter().map(|o| o.id).collect::<Vec<_>>();
    // Equal z: the later in the document is drawn over the earlier.
    let front_first: Vec<FloatId> = ids.iter().rev().copied().collect();
    for _ in 0..4 {
        assert_eq!(list(&mut e), front_first);
    }
    select(&mut e, ids[0].0);
    assert_eq!(e.object_props(fonts()).and_then(|p| p.stack_index), Some(2));
}

#[test]
fn a_column_float_dragged_to_the_other_column_keeps_its_place() {
    let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
    let float = anchor(0, 40, 100, 100, 1, false, 5, &star)
        .replace(
            r#"<wp:positionH relativeFrom="page"><wp:posOffset>0</wp:posOffset></wp:positionH>"#,
            r#"<wp:positionH relativeFrom="column"><wp:align>center</wp:align></wp:positionH>"#,
        )
        .replace(r#"<wp:positionV relativeFrom="page">"#, r#"<wp:positionV relativeFrom="paragraph">"#);
    assert!(float.contains("center") && float.contains(r#"positionV relativeFrom="paragraph""#), "{float}");
    let mut paras: String =
        (0..4).map(|k| format!("<w:p><w:r><w:t>Left {k}</w:t></w:r>{}</w:p>", if k == 0 { float.as_str() } else { "" })).collect();
    paras.push_str(r#"<w:p><w:r><w:br w:type="column"/></w:r></w:p>"#);
    paras.extend((0..4).map(|k| format!("<w:p><w:r><w:t>Right {k}</w:t></w:r></w:p>")));
    let b = with_document(&bytes(&paras).expect("a package"), |d| {
        d.replace("<w:sectPr><w:pgSz", r#"<w:sectPr><w:cols w:num="2" w:space="720"/><w:pgSz"#)
    });
    let mut e = Editor::open(&b, fonts()).expect("opens");
    e.complete(fonts());
    let f = FloatId(all_floats(&e)[0]);
    select(&mut e, f.0);
    let before = drawn(&mut e);
    assert!((before[0].0[0] - 1300).abs() <= 5, "centred in the left column (72..288): {before:?}");
    let v = e.view(0..1);
    let (_, [ax, ay]) = v.anchor.expect("the anchor shows");
    let u = 0.75 / e.viewport.zoom.max(0.1);
    let ring = [(ax - 18.0 * u).max(10.0 * u), ay + 8.0 * u];
    let target = body_para(&e, 6);
    let geo = e.pages.page(0, &e.index).expect("page 0");
    let line = geo.lines.iter().find(|l| l.shows(target)).expect("its line");
    let (x0, _) = line.span_of(target).expect("its span");
    assert!(x0 > 300.0, "in the right column: {x0}");
    let to = [x0 + 2.0, line.top + 2.0];
    drag(&mut e, ring, to, NONE);
    assert_eq!(e.index().float_anchor(f), Some(target), "anchored in the right column");
    assert_eq!(drawn(&mut e), before, "drawn where it was");
    let fl = e.float_of(f).expect("the float");
    assert_eq!((fl.h_align, fl.relative.pos[0]), (None, None), "placed by an offset");
    // 130 from the page's edge is 194 left of the right column (324).
    assert!((fl.h_offset + 194.0).abs() < 0.1, "from the right column: {}", fl.h_offset);
}

fn protect_read_only(e: &mut Editor) {
    use crate::api::command::{DocumentProtection, ProtectionKind};
    let read_only = DocumentProtection { kind: Some(ProtectionKind::ReadOnly), enforce: Some(true), ..Default::default() };
    assert!(changed(&e.execute(Command::SetDocumentProtection(read_only), fonts())));
}

#[test]
fn object_property_setters_respect_protection() {
    let mut e = with_inserted_picture();
    protect_read_only(&mut e);
    select_first_inline_picture(&mut e);
    let was = selected_picture(&e).cloned().expect("the picture");
    assert!(!changed(&e.set_object_name("Renamed", fonts())), "an inline picture keeps its name");
    assert!(!changed(&e.set_object_title("Title", fonts())));
    assert!(!changed(&e.set_lock_aspect(!was.lock_aspect, fonts())));
    assert_eq!(selected_picture(&e), Some(&was));
    let mut e = editor(&format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>",
        anchor(100, 300, 100, 100, 1, false, 5, &sp(0, 0, 100, 100, "rect", "", "FF0000"))
    ));
    protect_read_only(&mut e);
    let f = all_floats(&e)[0];
    select(&mut e, f);
    let was = e.float_of(FloatId(f)).cloned();
    assert!(!changed(&e.set_object_name("Renamed", fonts())), "a float keeps its name");
    assert!(!changed(&e.set_object_title("Title", fonts())));
    assert!(!changed(&e.set_lock_aspect(true, fonts())));
    assert_eq!(e.float_of(FloatId(f)).cloned(), was);
}

#[test]
fn reset_wrap_points_respects_protection() {
    let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(100, 300, 100, 100, 1, false, 5, &star)));
    let f = all_floats(&e)[0];
    select(&mut e, f);
    assert!(changed(&e.execute(Command::SetObjectWrap(ObjectWrap::Tight), fonts())));
    let square = [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)];
    assert!(changed(&e.set_wrap_points(&square, fonts())));
    protect_read_only(&mut e);
    select(&mut e, f);
    let before = e.selected_wrap_polygon().expect("a polygon");
    assert_eq!((before.points.len(), before.edited), (4, true));
    assert!(!changed(&e.reset_wrap_points(fonts())));
    assert_eq!(e.selected_wrap_polygon(), Some(before), "the edited polygon stays, every point");
}

#[test]
fn set_wrap_points_needs_an_editable_anchor_and_three_points() {
    let star = anchor(100, 300, 100, 100, 1, false, 5, &sp(0, 0, 100, 100, "star5", "", "FF0000"));
    let locked = format!(
        r#"<w:p><w:sdt><w:sdtPr><w:id w:val="5"/><w:lock w:val="sdtContentLocked"/></w:sdtPr><w:sdtContent><w:r><w:t>Text</w:t></w:r>{star}</w:sdtContent></w:sdt></w:p>"#
    );
    let mut e = editor(&locked);
    let f = all_floats(&e)[0];
    select(&mut e, f);
    let square = [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)];
    assert!(!changed(&e.set_wrap_points(&square, fonts())), "its anchor is locked");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{star}</w:p>"));
    let f = all_floats(&e)[0];
    select(&mut e, f);
    assert!(!changed(&e.set_wrap_points(&[(0.0, 0.0), (1.0, 1.0), (0.0, 0.0)], fonts())), "two vertices, closed");
    assert!(!changed(&e.set_wrap_points(&[(-50.0, 0.0), (1.0, 1.0), (-90.0, 0.0)], fonts())), "two once clamped");
    assert!(changed(&e.set_wrap_points(&[(0.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.0, 0.0)], fonts())), "a closed triangle");
    assert_eq!(e.selected_wrap_polygon().map(|p| p.points.len()), Some(3));
}

#[test]
fn select_objects_takes_a_turned_float_by_the_box_it_is_drawn_in() {
    let mut e = two_shapes();
    let star = all_floats(&e)[0];
    select(&mut e, star);
    assert!(changed(&e.execute(Command::SetObjectRotation(45.0), fonts())));
    e.complete(fonts());
    let geo = e.pages.page(0, &e.index).expect("page 0");
    let s = geo.stories.iter().find(|s| s.float == Some(FloatId(star))).expect("the star's story");
    let (laid, drawn) = (s.rect, s.turn.rect(s.rect));
    assert!(drawn[0] < laid[0] - 1.0, "turned out past its box: {laid:?} {drawn:?}");
    let caret = Selection::Caret(Position::text(StoryId::Body, body_para(&e, 0), Unit(0)));
    e.set_selection(caret);
    e.set_select_objects(true);
    drag(&mut e, [laid[0] - 1.0, laid[1] - 1.0], [laid[2] + 1.0, laid[3] + 1.0], NONE);
    assert!(e.selected_floats().is_empty(), "its corners stick out of the rectangle");
    drag(&mut e, [drawn[0] - 1.0, drawn[1] - 1.0], [drawn[2] + 1.0, drawn[3] + 1.0], NONE);
    assert_eq!(e.selected_floats(), vec![FloatId(star)]);
}

/// The inserted picture floating beside a shape, grouped, and the group's picture child selected.
fn grouped_picture_child() -> (Editor, FloatId) {
    let mut e = with_inserted_picture();
    assert!(changed(&e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts())));
    let pic = FloatId(all_floats(&e)[0]);
    let _ = e.execute(Command::Escape, fonts());
    assert!(changed(
        &e.execute(Command::InsertShape(ShapeInsert { preset: ShapePreset::from_name("rect").unwrap(), width: None, height: None }), fonts())
    ));
    let rect = FloatId(*all_floats(&e).iter().find(|&&f| f != pic.0).expect("the shape"));
    assert!(e.select_listed(pic, &[], false) == Outcome::Moved);
    assert!(e.select_listed(rect, &[], true) == Outcome::Moved);
    assert!(changed(&e.group_objects(fonts())));
    let g = FloatId(all_floats(&e)[0]);
    let list = e.object_list(0, fonts());
    let child = list[0].children.iter().find(|c| c.kind == crate::api::state::ObjectKind::Picture).expect("the picture child").path.clone();
    assert!(e.select_listed(g, &child, false) == Outcome::Moved);
    (e, g)
}

#[test]
fn a_group_childs_picture_gives_its_own_size_and_leaves_the_groups_lock() {
    let (mut e, g) = grouped_picture_child();
    assert!(e.object_image().is_some());
    assert!(e.object_props(fonts()).is_some_and(|p| p.original_size.is_some()), "the child picture's own size");
    let lock = e.float_of(g).map(|f| f.lock_aspect);
    assert!(!changed(&e.set_lock_aspect(!lock.unwrap_or(false), fonts())), "the child resizes with its group");
    assert_eq!(e.float_of(g).map(|f| f.lock_aspect), lock, "the group's lock stays");
}

#[test]
fn a_text_boxs_lock_aspect_keeps_its_corners_in_proportion() {
    let rect = sp(0, 0, 150, 60, "rect", "", "FFFFFF").replace(r#"<wps:cNvSpPr/>"#, r#"<wps:cNvSpPr txBox="1"/>"#);
    let tb = rect.replace("<wps:bodyPr/>", r#"<wps:txbx><w:txbxContent><w:p/></w:txbxContent></wps:txbx><wps:bodyPr/>"#);
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(72, 300, 150, 60, 1, false, 1, &tb)));
    let f = FloatId(all_floats(&e)[0]);
    select(&mut e, f.0);
    assert!(!e.float_locked(f));
    assert!(changed(&e.set_lock_aspect(true, fonts())));
    assert!(e.float_locked(f), "a corner drag keeps its proportions");
}

#[test]
fn a_new_objects_name_numbers_past_a_renamed_one() {
    let mut e = two_shapes();
    let insert = |e: &mut Editor| {
        let _ = e.execute(Command::Escape, fonts());
        assert!(changed(
            &e.execute(Command::InsertShape(ShapeInsert { preset: ShapePreset::from_name("rect").unwrap(), width: None, height: None }), fonts())
        ));
        e.object_props(fonts()).expect("props").name
    };
    let first = insert(&mut e);
    let (base, n) = first.rsplit_once(' ').expect("numbered");
    let taken = format!("{base} {}", n.parse::<u32>().expect("a number") + 1);
    let star = all_floats(&e)[0];
    select(&mut e, star);
    assert!(changed(&e.set_object_name(&taken, fonts())));
    assert_ne!(insert(&mut e), taken, "not the renamed object's name");
}

#[test]
fn restacking_in_front_of_the_highest_rank_renumbers_the_layer() {
    let r = |x: i64, id: u32, z: u32| anchor(x, 300, 100, 100, id, false, z, &sp(0, 0, 100, 100, "rect", "", "FF0000"));
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}{}{}</w:p>", r(72, 1, 5), r(200, 2, u32::MAX - 1), r(330, 3, u32::MAX)));
    let ids: Vec<FloatId> = all_floats(&e).into_iter().map(FloatId).collect();
    let order = |e: &mut Editor| e.object_list(0, fonts()).iter().map(|o| o.id).collect::<Vec<_>>();
    assert_eq!(order(&mut e), [ids[2], ids[1], ids[0]]);
    assert!(changed(&e.restack_object(ids[0], Some(ids[2]), fonts())));
    assert_eq!(order(&mut e), [ids[0], ids[2], ids[1]], "in front of the top one");
    assert!(changed(&e.restack_object(ids[1], Some(ids[0]), fonts())));
    assert_eq!(order(&mut e), [ids[1], ids[0], ids[2]]);
}

/// An inline picture `w` by `h` points: its `xfrm` attributes, `wp:effectExtent`, preset geometry and effects as given.
fn inline_picture(w: i64, h: i64, xfrm: &str, extent: &str, prst: &str, effects: &str) -> Editor {
    let (cx, cy) = (w * PT, h * PT);
    let pic = format!(
        r#"<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="1" name="P"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm{xfrm}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="{prst}"><a:avLst/></a:prstGeom>{effects}</pic:spPr></pic:pic>"#
    );
    let mut e = editor(&format!(
        r#"<w:p><w:r><w:t>A</w:t></w:r><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>{extent}<wp:docPr id="1" name="Picture 1"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">{pic}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
    ));
    select_first_inline_picture(&mut e);
    e
}

#[test]
fn turning_a_picture_read_without_effect_extents_leaves_no_reach_behind() {
    let mut e = inline_picture(200, 100, r#" rot="5400000""#, "", "rect", "");
    assert_eq!(first_inline_picture(&e).map(|p| p.effect), Some([0.0; 4]), "none read");
    assert!(changed(&e.execute(Command::SetObjectRotation(0.0), fonts())));
    assert_eq!(first_inline_picture(&e).map(|p| p.effect), Some([0.0; 4]), "unturned: its own box");
    let mut e = inline_picture(200, 100, r#" rot="5400000""#, "", "rect", "");
    assert!(changed(&e.execute(Command::SetObjectRotation(180.0), fonts())));
    let p = first_inline_picture(&e).expect("the picture");
    assert!(p.effect.iter().all(|v| v.abs() < 1e-6), "a half turn reaches no further: {:?}", p.effect);
}

#[test]
fn reset_picture_drops_its_effects_extents() {
    let shadow = r#"<a:effectLst><a:outerShdw blurRad="63500" dist="0" dir="0"><a:srgbClr val="000000"/></a:outerShdw></a:effectLst>"#;
    let mut e = inline_picture(72, 72, "", r#"<wp:effectExtent l="63500" t="63500" r="63500" b="63500"/>"#, "rect", shadow);
    assert!(first_inline_picture(&e).is_some_and(|p| p.effect[0] > 1.0), "the shadow's reach");
    assert!(changed(&e.reset_picture(false, fonts())));
    let p = first_inline_picture(&e).expect("the picture");
    assert_eq!(p.effect, [0.0; 4], "no effect reaches past its box");
    assert!(saved_xml(&e).contains(r#"<wp:effectExtent l="0" t="0" r="0" b="0"/>"#), "{}", saved_xml(&e));
}

#[test]
fn a_picture_cut_to_a_shape_takes_its_adjustments() {
    let mut e = inline_picture(144, 72, "", "", "roundRect", "");
    let adjust = |e: &Editor| first_inline_picture(e).and_then(|p| p.shape.as_deref()?.geometry().map(|g| g.adjust_values()));
    assert_ne!(adjust(&e).and_then(|a| a.first().copied()), Some(30_000));
    assert!(changed(&e.set_shape_adjust(0, 30_000, fonts())));
    assert_eq!(adjust(&e).and_then(|a| a.first().copied()), Some(30_000));
}

/// A frame in the body (its paragraph `framePr` and its other paragraph properties), selected, with `styles`.
fn framed(frame_pr: &str, ppr: &str, styles: &str) -> (Editor, FloatId) {
    let body = format!(r#"<w:p><w:pPr>{ppr}{frame_pr}</w:pPr><w:r><w:t>FRAMED</w:t></w:r></w:p><w:p><w:r><w:t>Body text.</w:t></w:r></w:p>"#);
    let mut e = Editor::open(&super::services::package(&body, styles, ""), fonts()).expect("opens");
    e.complete(fonts());
    let f = FloatId(all_floats(&e)[0]);
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(f)));
    (e, f)
}

#[test]
fn a_frame_dragged_to_a_twip_that_reads_as_an_alignment_keeps_its_place() {
    let pr = r#"<w:framePr w:w="2880" w:wrap="around" w:vAnchor="page" w:hAnchor="page" w:x="1441" w:y="2881"/>"#;
    for at in [-0.25, -0.05, -0.65] {
        let (mut e, f) = framed(pr, "", "");
        let mut moved = e.float_of(f).cloned().expect("the frame");
        (moved.h_offset, moved.v_offset) = (at, at);
        assert!(changed(&e.commit_frame(f, &moved, "Move Object", fonts())), "{at}");
        let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
        let n = again.float_of(FloatId(all_floats(&again)[0])).cloned().expect("the frame");
        assert_eq!((n.h_align, n.v_align), (None, None), "{at}: a place, not an alignment");
        assert!((n.h_offset - at).abs() <= 0.06 && (n.v_offset - at).abs() <= 0.06, "{at}: ({}, {})", n.h_offset, n.v_offset);
    }
}

#[test]
fn allowing_overlap_overrides_a_styles_suppress_overlap() {
    use crate::api::command::FrameProperties;
    let style = r#"<w:style w:type="paragraph" w:styleId="Apart"><w:name w:val="Apart"/><w:pPr><w:suppressOverlap/></w:pPr></w:style>"#;
    let pr = r#"<w:framePr w:w="2880" w:wrap="around" w:vAnchor="page" w:hAnchor="page" w:x="1441" w:y="2881"/>"#;
    let (mut e, f) = framed(pr, r#"<w:pStyle w:val="Apart"/>"#, style);
    assert!(e.float_of(f).is_some_and(|n| !n.allow_overlap), "the style keeps it apart");
    let props: FrameProperties = e.frame_properties(None).expect("frame properties");
    assert!(changed(&e.set_frame_with_overlap(props, Some(true), fonts())));
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert!(again.float_of(FloatId(all_floats(&again)[0])).is_some_and(|n| n.allow_overlap), "saved as allowed");
}

#[test]
fn a_live_drag_to_the_next_page_of_a_vertical_section_drops_as_a_plain_one() {
    use crate::api::command::TextFlow;
    let words = crate::layout::tests::words(300);
    let filler: String = (0..12).map(|_| format!("<w:p><w:r><w:t>{words}</w:t></w:r></w:p>")).collect();
    let shape = sp(0, 0, 60, 40, "rect", "", "FF0000");
    let body = format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>{filler}", anchor(100, 200, 60, 40, 1, false, 1, &shape));
    let run = |live: bool| {
        let mut e = editor(&body);
        e.execute(Command::SetSectionTextDirection(crate::api::command::SectionValue::here(TextFlow::TbRl)), fonts());
        e.complete(fonts());
        e.live_layout = live;
        let f = FloatId(all_floats(&e)[0]);
        let pages = e.page_geometries();
        assert!(pages.len() > 1, "two pages");
        let s = pages[0].1.stories.iter().find(|s| s.float == Some(f)).expect("drawn on the first page");
        assert_ne!(s.region, crate::layout::vertical::Turn::NONE, "its region is turned");
        let c = [(s.rect[0] + s.rect[2]) / 2.0, (s.rect[1] + s.rect[3]) / 2.0];
        let region = s.region;
        e.set_selection(Selection::Object(ObjectRef::Floating(f)));
        let (x, y) = e.stack().to_doc(0, c[0], c[1]).expect("on the stack");
        let alt = Mods { alt: true, ..NONE };
        e.pointer_input(PointerKind::Down, x, y, 1, alt, fonts());
        let (x, y) = e.stack().to_doc(1, c[0], c[1] + 30.0).expect("on the stack");
        e.pointer_input(PointerKind::Move, x, y, 0, alt, fonts());
        e.pointer_input(PointerKind::Move, x, y, 0, alt, fonts());
        e.pointer_input(PointerKind::Up, x, y, 0, alt, fonts());
        let fl = e.float_of(f).cloned().expect("the float");
        (region, fl.h_offset, fl.v_offset)
    };
    assert_eq!(run(true), run(false), "live layout offsets it as a plain drag does");
}
