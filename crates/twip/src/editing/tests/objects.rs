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
    e.execute(Command::Undo, fonts());
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
                e.execute(Command::Undo, fonts());
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
    doc.settings.compat_mode = 15;
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
