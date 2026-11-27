//! Shape editing tests

use super::super::Editor;
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::selection::{ObjectRef, Selection};
use super::fonts;
use crate::layout::FloatId;
use crate::layout::tests::shapes::{anchor, bytes, sp};

fn editor(body: &str) -> Editor {
    let mut e = Editor::open(&bytes(body).expect("a package"), fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn click(e: &mut Editor, x: f64, y: f64) -> Selection {
    let (dx, dy) = e.stack().to_doc(0, x, y).expect("on the stack");
    e.pointer_input(PointerKind::Down, dx, dy, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, dx, dy, 0, Mods::default(), fonts());
    e.selection()
}

fn floats(e: &Editor) -> Vec<u64> {
    let mut v = Vec::new();
    for s in &e.document().sections {
        for b in &s.blocks {
            if let crate::docx::Block::Para(p) = b {
                v.extend(p.inlines.iter().filter_map(|i| if let crate::docx::Inline::Float(f, _) = i { Some(f.id) } else { None }));
            }
        }
    }
    v
}

fn picked(s: Selection) -> Option<u64> {
    match s {
        Selection::Object(ObjectRef::Floating(FloatId(f))) => Some(f),
        _ => None,
    }
}

#[test]
fn a_press_selects_a_shape_on_its_ink_only() {
    let ellipse = sp(0, 0, 144, 144, "ellipse", "", "FF0000");
    let star = sp(0, 0, 144, 144, "star5", "", "FF0000").replace(r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:ln"#, "<a:noFill/><a:ln");
    let line = sp(0, 0, 200, 0, "line", "", "FF0000");
    let body = format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}{}{}</w:p>",
        anchor(72, 300, 144, 144, 1, false, 1, &ellipse),
        anchor(300, 300, 144, 144, 2, false, 2, &star),
        anchor(72, 500, 200, 0, 3, false, 3, &line)
    );
    let mut e = editor(&body);
    let id = |e: &Editor, n: usize| floats(e)[n];
    let (el, st, ln) = (id(&e, 0), id(&e, 1), id(&e, 2));
    assert_eq!(picked(click(&mut e, 144.0, 372.0)), Some(el));
    assert_eq!(picked(click(&mut e, 76.0, 304.0)), None, "outside the ellipse, inside its box");
    assert_eq!(picked(click(&mut e, 372.0, 301.0)), Some(st));
    assert_eq!(picked(click(&mut e, 372.0, 380.0)), None, "inside a shape without fill");
    assert_eq!(picked(click(&mut e, 150.0, 501.5)), Some(ln));
    assert_eq!(picked(click(&mut e, 150.0, 510.0)), None);
}

#[test]
fn a_rotated_shape_is_hit_where_it_is_drawn() {
    let bar = sp(0, 0, 200, 20, "rect", r#" rot="5400000""#, "00FF00");
    let body = format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(72, 400, 200, 20, 1, false, 1, &bar));
    let mut e = editor(&body);
    let f = floats(&e)[0];
    assert_eq!(picked(click(&mut e, 172.0, 330.0)), Some(f), "on the turned bar");
    assert_eq!(picked(click(&mut e, 90.0, 410.0)), None, "where the bar was before turning");
}

fn group_and_ellipse() -> Editor {
    use crate::layout::tests::shapes::PT;
    let group = format!(
        r#"<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{h}"/><a:chOff x="0" y="0"/><a:chExt cx="{w}" cy="{h}"/></a:xfrm></wpg:grpSpPr>{}{}</wpg:wgp>"#,
        sp(0, 0, 80, 80, "rect", "", "0000FF"),
        sp(120, 0, 80, 80, "rect", "", "00FF00"),
        w = 200 * PT,
        h = 100 * PT,
    );
    let ellipse = sp(0, 0, 100, 100, "ellipse", "", "FF0000");
    let body = format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}{}</w:p>",
        anchor(72, 300, 200, 100, 1, false, 1, &group),
        anchor(320, 300, 100, 100, 2, false, 2, &ellipse)
    );
    editor(&body)
}

#[test]
fn a_second_press_into_a_group_selects_its_child_and_esc_goes_back() {
    use super::super::commands::Command;
    let mut e = group_and_ellipse();
    let g = floats(&e)[0];
    assert_eq!(picked(click(&mut e, 100.0, 330.0)), Some(g));
    assert_eq!(e.selected_child(), None, "the first press selects the group");
    click(&mut e, 100.0, 330.0);
    assert_eq!(e.selected_child(), Some(&[0u16][..]));
    click(&mut e, 240.0, 330.0);
    assert_eq!(e.selected_child(), Some(&[1u16][..]), "another child of the selected group");
    e.execute(Command::Escape, fonts());
    assert_eq!((picked(e.selection()), e.selected_child()), (Some(g), None), "Esc: the group");
    e.execute(Command::Escape, fonts());
    assert_eq!(picked(e.selection()), None, "Esc again: the text");
}

#[test]
fn shift_or_ctrl_and_a_press_add_and_remove_floats() {
    let mut e = group_and_ellipse();
    let ids: Vec<FloatId> = floats(&e).into_iter().map(FloatId).collect();
    click(&mut e, 100.0, 330.0);
    let with = |e: &mut Editor, x: f64, y: f64, m: Mods| {
        let (dx, dy) = e.stack().to_doc(0, x, y).expect("on the stack");
        e.pointer_input(PointerKind::Down, dx, dy, 1, m, fonts());
        e.pointer_input(PointerKind::Up, dx, dy, 0, m, fonts());
    };
    with(&mut e, 370.0, 350.0, Mods { shift: true, ..Mods::default() });
    assert_eq!(e.selected_floats(), ids);
    with(&mut e, 370.0, 350.0, Mods { ctrl: true, ..Mods::default() });
    assert_eq!(e.selected_floats(), ids[..1], "a second Ctrl+press takes it out");
    with(&mut e, 370.0, 350.0, Mods { shift: true, ..Mods::default() });
    click(&mut e, 370.0, 350.0);
    assert_eq!(e.selected_floats(), ids[1..]);
}

fn drag(e: &mut Editor, from: [f64; 2], to: [f64; 2], mods: Mods) {
    let doc = |e: &Editor, p: [f64; 2]| e.stack().to_doc(0, p[0], p[1]).expect("on the stack");
    let (a, b) = (doc(e, from), doc(e, to));
    e.pointer_input(PointerKind::Down, a.0, a.1, 1, mods, fonts());
    for k in 1..=8 {
        let t = f64::from(k) / 8.0;
        e.pointer_input(PointerKind::Move, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, 0, mods, fonts());
    }
    e.pointer_input(PointerKind::Up, b.0, b.1, 0, mods, fonts());
}

fn same_as_full(e: &mut Editor) {
    e.complete(fonts());
    let ours = crate::layout::dump::page_text(&e.snapshot(), e.document());
    let fresh = crate::layout::dump::page_text(&crate::layout::layout(e.document(), fonts()), e.document());
    assert!(ours == fresh, "incremental layout differs from a full layout");
}

fn shape_of(e: &Editor) -> crate::drawing::Shape {
    let f = floats(e)[0];
    e.document().sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                crate::docx::Inline::Float(fl, _) if fl.id == f => fl.shape.as_deref().cloned(),
                _ => None,
            }),
            _ => None,
        })
        .expect("the float's shape")
}

#[test]
fn the_rotate_handle_turns_a_shape_and_shift_snaps_it() {
    use super::super::commands::Command;
    let square = sp(0, 0, 100, 100, "rect", "", "0000FF");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(200, 300, 100, 100, 1, false, 1, &square)));
    click(&mut e, 250.0, 350.0);
    let frame = e.view(0..1).shapes.first().cloned().expect("a shape frame");
    assert!((frame.rotate[0] - 250.0).abs() < 1e-6 && (frame.rotate[1] - (300.0 - 18.0)).abs() < 1e-6, "{frame:?}");
    drag(&mut e, frame.rotate, [350.0, 345.0], Mods { shift: true, ..Mods::default() });
    assert_eq!(shape_of(&e).xfrm.rot.0, 90 * 60000);
    same_as_full(&mut e);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(shape_of(&e).xfrm.rot.0, 0, "one undo step");
}

#[test]
fn an_adjust_handle_drags_within_its_limits() {
    use super::super::commands::Command;
    let rr = sp(0, 0, 200, 100, "roundRect", "", "00FF00");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(100, 300, 200, 100, 1, false, 1, &rr)));
    click(&mut e, 200.0, 350.0);
    let frame = e.view(0..1).shapes.first().cloned().expect("a shape frame");
    let h = frame.adjust[0];
    drag(&mut e, h, [130.0, h[1]], Mods::default());
    let adj = |e: &Editor| shape_of(e).geometry().expect("a geometry").adjust_values()[0];
    assert!((adj(&e) - 30000).abs() <= 100, "{}", adj(&e));
    same_as_full(&mut e);
    let h = e.view(0..1).shapes.first().cloned().expect("a shape frame").adjust[0];
    drag(&mut e, h, [290.0, h[1]], Mods::default());
    assert_eq!(adj(&e), 50000);
    e.execute(Command::Undo(Default::default()), fonts());
    assert!((adj(&e) - 30000).abs() <= 100, "one undo step");
}

fn box_text(e: &Editor) -> Option<String> {
    let f = floats(e)[0];
    e.document().sections[0].blocks.iter().find_map(|b| match b {
        crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| match i {
            crate::docx::Inline::Float(fl, _) if fl.id == f => fl.text_box.as_ref().map(|tb| {
                tb.blocks
                    .iter()
                    .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
                    .flat_map(|p| p.inlines.iter())
                    .filter_map(|i| if let crate::docx::Inline::Text(t, _) = i { Some(t.as_str()) } else { None })
                    .collect()
            }),
            _ => None,
        }),
        _ => None,
    })
}

#[test]
fn typing_into_a_shape_gives_it_a_story_and_a_line_takes_none() {
    use super::super::commands::Command;
    let ellipse = sp(0, 0, 144, 144, "ellipse", "", "FF0000");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(72, 300, 144, 144, 1, false, 1, &ellipse)));
    click(&mut e, 144.0, 372.0);
    e.execute(Command::InsertText("Hi".into()), fonts());
    assert_eq!(box_text(&e).as_deref(), Some("Hi"));
    assert!(matches!(e.selection().focus().map(|p| p.story), Some(super::super::position::StoryId::TextBox(_))), "the caret is in the shape");
    assert!(matches!(shape_of(&e).sp().map(|s| &s.text), Some(crate::drawing::ShapeText::Story)));
    same_as_full(&mut e);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(box_text(&again).as_deref(), Some("Hi"));
    let line = sp(0, 0, 200, 0, "line", "", "FF0000");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(72, 500, 200, 0, 1, false, 1, &line)));
    click(&mut e, 150.0, 500.5);
    assert!(picked(e.selection()).is_some());
    e.execute(Command::InsertText("x".into()), fonts());
    assert_eq!(box_text(&e), None);
}

fn selected_box(e: &mut Editor) -> [f64; 4] {
    e.complete(fonts());
    let Selection::Object(ObjectRef::Floating(f)) = e.selection() else { panic!("no float selected: {:?}", e.selection()) };
    let snap = e.snapshot();
    let items = crate::layout::traverse::paint_float(&snap.pages()[0], f);
    items.iter().find_map(|it| if let crate::layout::fragment::Item::Shape(s) = it { Some(s.rect) } else { None }).expect("drawn")
}

#[test]
fn a_click_or_a_drag_inserts_a_shape_there() {
    use super::super::commands::Command;
    use crate::drawing::NewShape;
    use crate::drawing::geom::Preset;
    let text = crate::layout::tests::words(120);
    let mut e = editor(&format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p><w:p><w:r><w:t>{text}</w:t></w:r></w:p>"));
    let near = |a: [f64; 4], b: [f64; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.1);
    e.begin_shape_insert(NewShape::Shape(Preset::Star5));
    drag(&mut e, [100.0, 200.0], [220.0, 260.0], Mods::default());
    let r = selected_box(&mut e);
    assert!(near(r, [100.0, 200.0, 220.0, 260.0]), "{r:?}");
    assert_eq!(shape_of(&e).sp().and_then(|s| s.geom.preset()), Some(Preset::Star5));
    same_as_full(&mut e);
    e.begin_shape_insert(NewShape::Shape(Preset::Ellipse));
    drag(&mut e, [300.0, 400.0], [360.0, 420.0], Mods { shift: true, ..Mods::default() });
    assert!(near(selected_box(&mut e), [300.0, 400.0, 360.0, 460.0]), "{:?}", selected_box(&mut e));
    e.begin_shape_insert(NewShape::Shape(Preset::Rect));
    click(&mut e, 150.0, 500.0);
    assert!(near(selected_box(&mut e), [150.0, 500.0, 222.0, 572.0]), "{:?}", selected_box(&mut e));
    assert_eq!(floats(&e).len(), 3);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(floats(&e).len(), 2, "one undo step");
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(floats(&again).len(), 2);
    let style = shape_of(&again).sp().and_then(|s| s.style.clone()).expect("style references");
    assert_eq!(style.fill.map(|f| f.0), Some(1));
}

fn shape_by_id(e: &Editor, f: u64) -> crate::docx::Float {
    e.document().sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                crate::docx::Inline::Float(fl, _) if fl.id == f => Some((**fl).clone()),
                _ => None,
            }),
            _ => None,
        })
        .expect("the float")
}

#[test]
fn format_operations_change_the_selected_shapes_in_one_step_each() {
    use super::super::commands::Command;
    use super::super::shapes::Arrange;
    use crate::drawing::colour::Colour;
    use crate::drawing::geom::Preset;
    use crate::drawing::paint::Fill;
    let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
    let ellipse = sp(0, 0, 100, 100, "ellipse", "", "00FF00");
    let body = format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}{}</w:p>",
        anchor(100, 300, 100, 100, 1, false, 5, &star),
        anchor(150, 320, 100, 100, 2, false, 6, &ellipse)
    );
    let mut e = editor(&body);
    let [a, b] = floats(&e)[..] else { panic!() };
    // Change shape
    click(&mut e, 140.0, 350.0);
    assert_eq!(picked(e.selection()), Some(a));
    assert!(matches!(e.set_shape_preset(Preset::Hexagon, fonts()), super::super::Outcome::Changed(_)));
    let s = shape_by_id(&e, a).shape.expect("a shape");
    assert_eq!(s.sp().and_then(|s| s.geom.preset()), Some(Preset::Hexagon));
    assert!(matches!(&s.sp().unwrap().fill, Some(Fill::Solid(c)) if *c == Colour::srgb(0xFF0000)));
    // Fill both
    let (dx, dy) = e.stack().to_doc(0, 220.0, 380.0).expect("on the stack");
    e.pointer_input(PointerKind::Down, dx, dy, 1, Mods { shift: true, ..Mods::default() }, fonts());
    e.pointer_input(PointerKind::Up, dx, dy, 0, Mods::default(), fonts());
    assert_eq!(e.selected_floats().len(), 2);
    e.set_shape_fill(Some(Fill::Solid(Colour::srgb(0x0000FF))), fonts());
    let blue = |e: &Editor, f| matches!(&shape_by_id(e, f).shape.unwrap().sp().unwrap().fill, Some(Fill::Solid(c)) if *c == Colour::srgb(0x0000FF));
    assert!(blue(&e, a) && blue(&e, b));
    assert_eq!(e.selected_floats().len(), 2, "the selection stays");
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(!blue(&e, a) && !blue(&e, b), "one undo step");
    same_as_full(&mut e);
    // Flip and rotate
    click(&mut e, 140.0, 350.0);
    click(&mut e, 400.0, 600.0);
    click(&mut e, 140.0, 350.0);
    assert_eq!(e.selected_floats(), vec![crate::layout::FloatId(a)]);
    e.flip_object(true, fonts());
    e.set_object_rotation(30.0, fonts());
    let x = shape_by_id(&e, a).shape.unwrap().xfrm;
    assert_eq!((x.flip_h, x.rot.0), (true, 30 * 60000));
    // Arrange
    e.arrange_object(Arrange::ToFront, fonts());
    assert!(shape_by_id(&e, a).z > shape_by_id(&e, b).z);
    e.arrange_object(Arrange::Backward, fonts());
    assert!(shape_by_id(&e, a).z < shape_by_id(&e, b).z);
    e.arrange_object(Arrange::BehindText, fonts());
    assert!(shape_by_id(&e, a).behind);
    same_as_full(&mut e);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&bytes, fonts()).expect("reopens");
    let (s, t) = (shape_by_id(&again, floats(&again)[0]), shape_by_id(&e, a));
    assert_eq!((s.z, s.behind, s.shape.as_ref().map(|s| s.xfrm)), (t.z, t.behind, t.shape.as_ref().map(|s| s.xfrm)));
}

#[test]
fn a_shape_goes_in_line_with_text_and_floats_again() {
    use super::super::commands::Command;
    use crate::api::value::ObjectWrap;
    let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(100, 300, 100, 100, 1, false, 5, &star)));
    click(&mut e, 150.0, 350.0);
    let out = e.execute(Command::SetObjectWrap(ObjectWrap::Inline), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let inline_shapes = |e: &Editor| -> Vec<crate::drawing::Shape> {
        e.document().sections[0]
            .blocks
            .iter()
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
            .flat_map(|p| p.inlines.iter())
            .filter_map(|i| if let crate::docx::Inline::Picture(p, _) = i { p.shape.as_deref().cloned() } else { None })
            .collect()
    };
    assert_eq!((floats(&e).len(), inline_shapes(&e).len()), (0, 1));
    same_as_full(&mut e);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut again = Editor::open(&bytes, fonts()).expect("reopens");
    let back = inline_shapes(&again);
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].sp().and_then(|s| s.geom.preset()), Some(crate::drawing::geom::Preset::Star5));
    let out = e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert_eq!(shape_of(&e).sp().and_then(|s| s.geom.preset()), Some(crate::drawing::geom::Preset::Star5));
    again.complete(fonts());
}

fn drawn_boxes(e: &mut Editor) -> Vec<[i64; 4]> {
    e.complete(fonts());
    let snap = e.snapshot();
    let mut v: Vec<[i64; 4]> = crate::layout::traverse::paint_items(&snap.pages()[0])
        .iter()
        .filter_map(|it| if let crate::layout::fragment::Item::Shape(s) = it { Some(s.rect.map(|v| (v * 10.0).round() as i64)) } else { None })
        .collect();
    v.sort();
    v
}

#[test]
fn group_and_ungroup_keep_every_shape_where_it_is() {
    use super::super::commands::Command;
    let star = sp(0, 0, 100, 100, "star5", "", "FF0000");
    let ellipse = sp(0, 0, 100, 60, "ellipse", "", "00FF00");
    let body = format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}{}</w:p>",
        anchor(100, 300, 100, 100, 1, false, 5, &star),
        anchor(260, 340, 100, 60, 2, false, 6, &ellipse)
    );
    let mut e = editor(&body);
    let was = drawn_boxes(&mut e);
    click(&mut e, 150.0, 350.0);
    let (dx, dy) = e.stack().to_doc(0, 310.0, 370.0).expect("on the stack");
    e.pointer_input(PointerKind::Down, dx, dy, 1, Mods { shift: true, ..Mods::default() }, fonts());
    e.pointer_input(PointerKind::Up, dx, dy, 0, Mods::default(), fonts());
    assert_eq!(e.selected_floats().len(), 2);
    let out = e.group_objects(fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert_eq!(floats(&e).len(), 1);
    assert_eq!(shape_of(&e).group().map(|g| g.children.len()), Some(2));
    assert_eq!(drawn_boxes(&mut e), was, "grouped, every shape where it was");
    same_as_full(&mut e);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut again = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(floats(&again).len(), 1);
    assert_eq!(drawn_boxes(&mut again), was);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(floats(&e).len(), 2, "one undo step");
    e.execute(Command::Redo(Default::default()), fonts());
    // Ungroup
    click(&mut e, 150.0, 350.0);
    let out = e.ungroup_objects(fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert_eq!(floats(&e).len(), 2);
    assert_eq!(e.selected_floats().len(), 2);
    assert_eq!(drawn_boxes(&mut e), was, "ungrouped, every shape where it was");
    same_as_full(&mut e);
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(floats(&e).len(), 1, "one undo step");
}

#[test]
fn insert_shape_command_selects_the_shape() {
    use super::super::commands::Command;
    let mut e = Editor::open(&bytes("<w:p><w:r><w:t>Text</w:t></w:r></w:p>").expect("a package"), fonts()).expect("opens");
    e.execute(Command::InsertText("Shapes".into()), fonts());
    let out = e.execute(
        Command::InsertShape(crate::api::drawing::ShapeInsert {
            preset: crate::api::drawing::ShapePreset::from_name("star5").unwrap(),
            width: None,
            height: None,
        }),
        fonts(),
    );
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert!(picked(e.selection()).is_some(), "{:?}", e.selection());
    assert!(e.object_selection().is_some(), "object state");
}

#[test]
fn dragging_a_group_child_moves_it_alone_and_grows_the_group() {
    use super::super::commands::Command;
    let mut e = group_and_ellipse();
    click(&mut e, 100.0, 330.0);
    click(&mut e, 100.0, 330.0);
    assert_eq!(e.selected_child(), Some(&[0u16][..]));
    let other = [1920, 3000, 2720, 3800];
    let free = Mods { alt: true, ..Mods::default() };
    drag(&mut e, [100.0, 330.0], [120.0, 340.0], free);
    assert_eq!(e.selected_child(), Some(&[0u16][..]), "the child stays selected");
    let boxes = drawn_boxes(&mut e);
    assert!(boxes.contains(&[920, 3100, 1720, 3900]) && boxes.contains(&other), "{boxes:?}");
    let frame = e.view(0..1).shapes.first().cloned().expect("the child's frame");
    assert_eq!(frame.rect.map(|v| (v * 10.0).round() as i64), [920, 3100, 1720, 3900]);
    same_as_full(&mut e);
    drag(&mut e, [130.0, 350.0], [130.0, 380.0], free);
    let boxes = drawn_boxes(&mut e);
    assert!(boxes.contains(&[920, 3400, 1720, 4200]) && boxes.contains(&other), "{boxes:?}");
    let g = shape_of(&e);
    assert_eq!(g.xfrm.ext[1].0, 120 * crate::layout::tests::shapes::PT);
    same_as_full(&mut e);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut again = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(drawn_boxes(&mut again), drawn_boxes(&mut e));
    e.execute(Command::Undo(Default::default()), fonts());
    let boxes = drawn_boxes(&mut e);
    assert!(boxes.contains(&[920, 3100, 1720, 3900]), "{boxes:?}");
    assert_eq!(shape_of(&e).xfrm.ext[1].0, 100 * crate::layout::tests::shapes::PT);
}

#[test]
fn with_tracking_a_shape_inserted_or_deleted_is_a_revision() {
    use super::super::commands::Command;
    use crate::drawing::{NewShape, geom::Preset};
    let float_runs = |e: &Editor| -> Vec<(u64, bool, bool)> {
        let mut v = Vec::new();
        for b in &e.document().sections[0].blocks {
            if let crate::docx::Block::Para(p) = b {
                v.extend(p.inlines.iter().filter_map(|i| match i {
                    crate::docx::Inline::Float(f, r) => Some((f.id, r.rev.ins.is_some(), r.rev.del.is_some())),
                    _ => None,
                }));
            }
        }
        v
    };
    let mut e = editor("<w:p><w:r><w:t>Text</w:t></w:r></w:p>");
    e.execute(Command::SetTrackChanges(true), fonts());
    let out = e.insert_shape_sized(NewShape::Shape(Preset::Rect), [72.0, 72.0], fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let runs = float_runs(&e);
    assert!(matches!(runs[..], [(_, true, false)]), "an insertion: {runs:?}");
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&bytes, fonts()).expect("reopens");
    assert!(matches!(float_runs(&again)[..], [(_, true, false)]), "{:?}", float_runs(&again));
    e.execute(Command::RejectAllRevisions(Default::default()), fonts());
    assert!(float_runs(&e).is_empty());
    let mut e = editor(&format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>",
        anchor(100, 300, 100, 100, 1, false, 1, &sp(0, 0, 100, 100, "rect", "", "0000FF"))
    ));
    e.execute(Command::SetTrackChanges(true), fonts());
    click(&mut e, 150.0, 350.0);
    e.execute(Command::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Forward, unit: None }), fonts());
    assert!(matches!(float_runs(&e)[..], [(_, false, true)]), "{:?}", float_runs(&e));
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&bytes, fonts()).expect("reopens");
    assert!(matches!(float_runs(&again)[..], [(_, false, true)]), "saved as a deletion: {:?}", float_runs(&again));
    e.execute(Command::AcceptAllRevisions(Default::default()), fonts());
    assert!(float_runs(&e).is_empty());
    e.insert_shape_sized(NewShape::Shape(Preset::Rect), [72.0, 72.0], fonts());
    e.execute(Command::Delete(crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Forward, unit: None }), fonts());
    assert!(float_runs(&e).is_empty(), "{:?}", float_runs(&e));
}

fn inline_text_shape() -> Editor {
    use crate::layout::tests::shapes::PT;
    let shape = r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="W" cy="H"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="DDEEFF"/></a:solidFill></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Inside</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr lIns="91440" tIns="45720" rIns="91440" bIns="45720"/></wps:wsp>"#
        .replace('W', &(144 * PT).to_string())
        .replace('H', &(72 * PT).to_string());
    let body = format!(
        r#"<w:p><w:r><w:t xml:space="preserve">Before </w:t></w:r><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{w}" cy="{h}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="1" name="Text Box 1"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">{shape}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r><w:r><w:t xml:space="preserve"> after</w:t></w:r></w:p>"#,
        w = 144 * PT,
        h = 72 * PT
    );
    editor(&body)
}

fn inline_text(e: &Editor) -> Option<String> {
    e.document().sections[0].blocks.iter().find_map(|b| match b {
        crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| match i {
            crate::docx::Inline::Picture(pic, _) => pic.text.as_ref().map(|t| {
                t.text_box
                    .blocks
                    .iter()
                    .filter_map(|b| if let crate::docx::Block::Para(q) = b { Some(q) } else { None })
                    .flat_map(|q| q.inlines.iter())
                    .filter_map(|i| if let crate::docx::Inline::Text(t, _) = i { Some(t.as_str()) } else { None })
                    .collect::<String>()
            }),
            _ => None,
        }),
        _ => None,
    })
}

#[test]
fn an_inline_shape_lays_out_its_text_and_takes_typing() {
    use super::super::commands::Command;
    let mut e = inline_text_shape();
    assert_eq!(inline_text(&e).as_deref(), Some("Inside"));
    let snap = e.snapshot();
    let runs: Vec<(String, f64, f64)> = crate::layout::traverse::paint_items(&snap.pages()[0])
        .iter()
        .filter_map(|it| match it {
            crate::layout::fragment::Item::Glyphs(g) => Some((g.chars.iter().map(|c| c.0).collect::<String>(), g.x, g.baseline)),
            _ => None,
        })
        .collect();
    let inside = runs.iter().find(|r| r.0.contains("Inside")).cloned().expect("the shape's text is drawn");
    let before = runs.iter().find(|r| r.0.contains("Before")).cloned().expect("the line's text");
    assert!(inside.1 > before.1 + 20.0 && inside.2 < before.2, "inside the shape, which sits on the line: {runs:?}");
    let (x, y) = (inside.1 + 2.0, inside.2 - 4.0);
    click(&mut e, x, y);
    click(&mut e, x, y);
    e.execute(Command::InsertText("X".into()), fonts());
    let text = inline_text(&e).expect("the story");
    assert!(text.contains('X') && text.len() == 7, "{text}");
    same_as_full(&mut e);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(inline_text(&again), Some(text));
}

#[test]
fn an_inline_shape_with_text_floats_and_goes_in_line_again_with_its_text() {
    use super::super::commands::Command;
    use crate::api::value::ObjectWrap;
    let mut e = inline_text_shape();
    let snap = e.snapshot();
    let shape = crate::layout::traverse::paint_items(&snap.pages()[0])
        .iter()
        .find_map(|it| if let crate::layout::fragment::Item::Shape(s) = it { Some(s.rect) } else { None })
        .expect("the shape");
    click(&mut e, shape[0] + 1.0, shape[1] + 1.0);
    assert!(matches!(e.selection(), Selection::Object(ObjectRef::Inline { .. })), "{:?}", e.selection());
    let out = e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert_eq!((box_text(&e).as_deref(), inline_text(&e)), (Some("Inside"), None), "a float with the text");
    same_as_full(&mut e);
    let out = e.execute(Command::SetObjectWrap(ObjectWrap::Inline), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert_eq!(inline_text(&e).as_deref(), Some("Inside"), "inline again with the text");
    same_as_full(&mut e);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(inline_text(&again).as_deref(), Some("Inside"));
}

fn linked_boxes() -> Editor {
    let story: String =
        (1..=8).map(|i| format!("<w:p><w:r><w:t>P{i} words to fill the boxes in turn, one after another.</w:t></w:r></w:p>")).collect();
    let rect = sp(0, 0, 150, 60, "rect", "", "FFFFFF");
    let head = rect.replace("<wps:bodyPr/>", &format!(r#"<wps:txbx id="1"><w:txbxContent>{story}</w:txbxContent></wps:txbx><wps:bodyPr/>"#));
    let link = |seq: u32| rect.replace("<wps:bodyPr/>", &format!(r#"<wps:linkedTxbx id="1" seq="{seq}"/><wps:bodyPr/>"#));
    editor(&format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}{}{}</w:p>",
        anchor(72, 300, 150, 60, 1, false, 1, &head),
        anchor(250, 300, 150, 60, 2, false, 2, &link(1)),
        anchor(72, 400, 150, 60, 3, false, 3, &link(2))
    ))
}

fn page_lines(e: &Editor) -> Vec<(String, f64, f64)> {
    let snap = e.snapshot();
    let mut v: Vec<(String, f64, f64)> = crate::layout::traverse::paint_items(&snap.pages()[0])
        .iter()
        .filter_map(|it| match it {
            crate::layout::fragment::Item::Glyphs(g) => Some((g.chars.iter().map(|c| c.0).collect::<String>(), g.x, g.baseline)),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.1.total_cmp(&b.1)));
    v
}

#[test]
fn linked_boxes_show_their_story_in_turn_and_take_typing_in_any_box() {
    use super::super::commands::Command;
    let mut e = linked_boxes();
    let lines = page_lines(&e);
    let at = |s: &str| lines.iter().find(|l| l.0.starts_with(s)).cloned().unwrap_or_else(|| panic!("{s} not drawn: {lines:?}"));
    let (p1, p2, p3) = (at("P1"), at("P2"), at("P3"));
    assert!(p1.1 < 100.0 && p1.2 < 330.0, "{p1:?}");
    assert!(p2.1 > 250.0 && p2.1 < 270.0 && p2.2 < 360.0, "{p2:?}");
    assert!(p3.1 < 100.0 && p3.2 > 400.0 && p3.2 < 430.0, "{p3:?}");
    assert_eq!(lines.iter().filter(|l| l.0.starts_with("P2")).count(), 1, "drawn once: {lines:?}");
    click(&mut e, p2.1 + 1.0, p2.2 - 3.0);
    click(&mut e, p2.1 + 1.0, p2.2 - 3.0);
    assert!(matches!(e.selection(), Selection::Caret(_)), "{:?}", e.selection());
    e.execute(Command::InsertText("X".into()), fonts());
    let text = box_text(&e).expect("the story");
    assert!(text.contains("XP2") || text.contains("PX2"), "{text}");
    same_as_full(&mut e);
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(box_text(&again), Some(text));
    assert_eq!(page_lines(&again).iter().map(|l| l.0.clone()).collect::<Vec<_>>(), page_lines(&e).iter().map(|l| l.0.clone()).collect::<Vec<_>>());
}

fn hover(e: &mut Editor, p: [f64; 2]) -> super::super::input::Cursor {
    let (x, y) = e.stack().to_doc(0, p[0], p[1]).expect("on the stack");
    e.pointer_input(PointerKind::Move, x, y, 0, Mods::default(), fonts());
    e.view(0..1).cursor
}

#[test]
fn the_rotate_and_adjust_handles_show_their_cursor_and_a_drag_shows_its_preview() {
    use super::super::input::Cursor;
    use super::super::view::Measure;
    let rr = sp(0, 0, 100, 100, "roundRect", "", "0000FF");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(200, 300, 100, 100, 1, false, 1, &rr)));
    click(&mut e, 250.0, 350.0);
    let frame = e.view(0..1).shapes.first().cloned().expect("a shape frame");
    assert_eq!(hover(&mut e, frame.rotate), Cursor::Rotate);
    assert_eq!(hover(&mut e, frame.adjust[0]), Cursor::Default);
    let doc = |e: &Editor, p: [f64; 2]| e.stack().to_doc(0, p[0], p[1]).expect("on the stack");
    let (a, b) = (doc(&e, frame.rotate), doc(&e, [350.0, 345.0]));
    e.pointer_input(PointerKind::Down, a.0, a.1, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Move, b.0, b.1, 0, Mods::default(), fonts());
    assert!(e.dragging(), "a rotate drag asks for frames");
    e.complete(fonts());
    let v = e.view(0..1);
    assert_eq!(v.cursor, Cursor::Rotate);
    assert!(v.original.is_some() && v.measures.iter().any(|(_, m)| matches!(m, Measure::Chip { .. })), "the ghost and the angle");
    let snap = e.snapshot();
    let drawn = crate::layout::traverse::paint_items(&snap.pages()[0]).into_iter().find_map(|it| match it {
        crate::layout::fragment::Item::Shape(s) => Some(s.rot),
        _ => None,
    });
    assert_eq!(drawn, Some(90.0), "the shape turns as it's dragged");
}

#[test]
fn a_turned_shape_shows_its_handles_turned_and_resizes_along_its_sides() {
    use super::super::input::Cursor;
    use super::super::view::SelectionView;
    let square = sp(0, 0, 100, 100, "rect", r#" rot="1800000""#, "0000FF");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(200, 300, 100, 100, 1, false, 1, &square)));
    click(&mut e, 250.0, 350.0);
    let outline = |e: &mut Editor| match e.view(0..1).selection {
        SelectionView::Object(o, true) => o[0],
        s => panic!("no handles: {s:?}"),
    };
    let o = outline(&mut e);
    assert!((o.rot - 30.0).abs() < 1e-6 && o.rect == [200.0, 300.0, 300.0, 400.0], "{o:?}");
    // The top handle, turned 30° about the centre; the bottom one opposite it stays put.
    let (s, c) = 30f64.to_radians().sin_cos();
    let top = [250.0 + 50.0 * s, 350.0 - 50.0 * c];
    let bottom = |o: super::super::view::Outline| {
        let [x0, y0, x1, y1] = o.rect;
        let (s, c) = o.rot.to_radians().sin_cos();
        let h = (y1 - y0) / 2.0;
        [(x0 + x1) / 2.0 - h * s, (y0 + y1) / 2.0 + h * c]
    };
    let before = bottom(o);
    assert_eq!(hover(&mut e, top), Cursor::ResizeNorthEastSouthWest, "the arrow turns with the handle");
    drag(&mut e, top, [top[0] + 20.0 * s, top[1] - 20.0 * c], Mods::default());
    e.complete(fonts());
    let o = outline(&mut e);
    assert!(((o.rect[2] - o.rect[0]) - 100.0).abs() < 0.1 && ((o.rect[3] - o.rect[1]) - 120.0).abs() < 0.1, "{o:?}");
    let after = bottom(o);
    assert!((after[0] - before[0]).abs() < 0.2 && (after[1] - before[1]).abs() < 0.2, "{before:?} {after:?}");
}

#[test]
fn a_quarter_turned_shape_resizes_along_its_own_length() {
    use super::super::view::SelectionView;
    let bar = sp(0, 0, 200, 20, "rect", r#" rot="5400000""#, "00FF00");
    let mut e = editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(72, 400, 200, 20, 1, false, 1, &bar)));
    click(&mut e, 172.0, 330.0);
    let outline = |e: &mut Editor| match e.view(0..1).selection {
        SelectionView::Object(o, true) => o[0],
        s => panic!("no handles: {s:?}"),
    };
    let o = outline(&mut e);
    let [x0, y0, x1, y1] = o.rect;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    assert!((o.rot - 90.0).abs() < 1e-6 && ((x1 - x0) - 200.0).abs() < 1e-6, "{o:?}");
    // Its right side, turned, is at the bottom; dragging it 30 down lengthens the bar and keeps its top.
    drag(&mut e, [cx, cy + 100.0], [cx, cy + 130.0], Mods::default());
    same_as_full(&mut e);
    let o = outline(&mut e);
    let [x0, y0, x1, y1] = o.rect;
    assert!(((x1 - x0) - 230.0).abs() < 0.1 && ((y1 - y0) - 20.0).abs() < 0.1, "{o:?}");
    assert!(((y0 + y1) / 2.0 - (x1 - x0) / 2.0 - (cy - 100.0)).abs() < 0.2, "the top stays: {o:?}");
}

#[test]
fn a_relative_size_from_the_other_axis_is_refused() {
    use super::super::commands::{Command, Outcome};
    use super::super::position::{Position, StoryId};
    use crate::api::command::{ObjectSize, RelativeSize, RelativeSizeReference};
    use crate::api::value::{NoneOr, ObjectWrap};
    let data = super::open("FX-OBJ.docx").document().images.last().map(|(_, d)| d.to_vec()).expect("an image");
    let mut e = Editor::open(&super::blank_body(r#"<w:p><w:r><w:t>text</w:t></w:r></w:p>"#), fonts()).expect("opens");
    let p = super::para_id(&e, 0);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, p, crate::layout::Unit(0))));
    e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(data), alt_text: None }), fonts());
    e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts());
    assert!(matches!(e.selection(), Selection::Object(ObjectRef::Floating(_))), "{:?}", e.selection());
    let rel = |to| Some(NoneOr::Value(RelativeSize { percent: 50.0, relative_to: to }));
    let refused = |o: &Outcome| matches!(o, Outcome::Unchanged(_) | Outcome::Refused(_));
    let mut size = |s: ObjectSize| e.execute(Command::SetObjectSize(s), fonts());
    let a = size(ObjectSize { relative_width: rel(RelativeSizeReference::TopMargin), ..Default::default() });
    assert!(refused(&a), "a width relative to the top margin: {a:?}");
    let b = size(ObjectSize { relative_height: rel(RelativeSizeReference::LeftMargin), ..Default::default() });
    assert!(refused(&b), "a height relative to the left margin: {b:?}");
    let c = size(ObjectSize {
        relative_width: rel(RelativeSizeReference::LeftMargin),
        relative_height: rel(RelativeSizeReference::TopMargin),
        ..Default::default()
    });
    assert!(matches!(c, Outcome::Changed(_)), "{c:?}");
}

#[test]
fn typing_in_a_text_box_keeps_its_anchor_paragraphs_shaping() {
    use super::super::commands::Command;
    let rect = sp(0, 0, 150, 60, "rect", "", "FFFFFF");
    let story = r#"<wps:txbx id="1"><w:txbxContent><w:p><w:r><w:t>Inside</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/>"#;
    let boxed = rect.replace("<wps:bodyPr/>", story);
    let words = "anchor words ".repeat(200);
    let mut e = editor(&format!("<w:p><w:r><w:t>{words}</w:t></w:r>{}</w:p>", anchor(72, 300, 150, 60, 1, false, 1, &boxed)));
    let inside = e.document().sections[0].blocks.iter().find_map(|b| match b {
        crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| i.story().and_then(|(_, tb)| tb.blocks.first())),
        _ => None,
    });
    let Some(crate::docx::Block::Para(inside)) = inside else { panic!("a text box paragraph") };
    let inside = crate::layout::ParaId(inside.id);
    e.select_range(inside, crate::layout::Unit(6), crate::layout::Unit(6), fonts());
    let (shaped, entries) = (e.layout.cache().shaped.counts().1, e.layout.cache().shaped.len());
    for _ in 0..20 {
        e.execute(Command::InsertText("x".into()), fonts());
    }
    let (shaped, entries) = (e.layout.cache().shaped.counts().1 - shaped, e.layout.cache().shaped.len() - entries);
    assert_eq!(box_text(&e).as_deref(), Some("Insidexxxxxxxxxxxxxxxxxxxx"));
    assert!(shaped <= 20 && entries <= 20, "{shaped} paragraphs shaped and {entries} entries kept for 20 keystrokes in the box");
    same_as_full(&mut e);
}

fn chain_texts(e: &Editor) -> Vec<(u64, crate::drawing::ShapeText, Option<u32>)> {
    let mut v = Vec::new();
    for b in &e.document().sections[0].blocks {
        if let crate::docx::Block::Para(p) = b {
            for i in &p.inlines {
                if let crate::docx::Inline::Float(f, _) = i
                    && let Some(sp) = f.shape.as_deref().and_then(|s| s.sp())
                {
                    v.push((f.id, sp.text.clone(), sp.chain));
                }
            }
        }
    }
    v
}

#[test]
fn break_link_leaves_the_boxes_after_it_a_chain_of_their_own_and_saves() {
    use crate::drawing::ShapeText as T;
    let mut e = linked_boxes();
    let ids = floats(&e);
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[0]))));
    let link = e.text_box_link().expect("in a chain");
    assert_eq!((link.index, link.count, link.next), (0, 3, Some(FloatId(ids[1]))));
    assert!(matches!(e.unlink_text_box(fonts()), super::super::Outcome::Changed(_)));
    assert_eq!(e.text_box_link(), None, "the first box is alone");
    let c = chain_texts(&e);
    assert!(matches!((&c[0].1, c[0].2), (T::Story, None)), "{c:?}");
    assert!(matches!((&c[1].1, c[1].2), (T::Story, Some(_))) && matches!(c[2].1, T::Linked { seq: 1, .. }), "{c:?}");
    same_as_full(&mut e);
    let lines = page_lines(&e);
    assert!(!lines.iter().any(|l| l.0.starts_with('P') && l.1 > 250.0), "the story stops in the first box: {lines:?}");
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut again = Editor::open(&bytes, fonts()).expect("reopens");
    again.set_selection(Selection::Object(ObjectRef::Floating(FloatId(floats(&again)[1]))));
    assert_eq!(again.text_box_link().map(|l| (l.index, l.count)), Some((0, 2)), "the rest are a chain");
    assert!(box_text(&again).is_some_and(|t| t.starts_with("P1")));
    // One undo joins them again; redo breaks them again.
    e.undo(true, fonts());
    assert_eq!(e.text_box_link().map(|l| l.count), Some(3));
    let _ = e.execute(super::super::commands::Command::Redo(Default::default()), fonts());
    assert_eq!(e.text_box_link(), None);
}

#[test]
fn create_link_takes_a_click_on_an_empty_text_box_and_the_text_flows_into_it() {
    let story: String =
        (1..=8).map(|i| format!("<w:p><w:r><w:t>P{i} words to fill the boxes in turn, one after another.</w:t></w:r></w:p>")).collect();
    let rect = sp(0, 0, 150, 60, "rect", "", "FFFFFF").replace(r#"<wps:cNvSpPr/>"#, r#"<wps:cNvSpPr txBox="1"/>"#);
    let full = rect.replace("<wps:bodyPr/>", &format!(r#"<wps:txbx><w:txbxContent>{story}</w:txbxContent></wps:txbx><wps:bodyPr/>"#));
    let empty = rect.replace("<wps:bodyPr/>", r#"<wps:txbx><w:txbxContent><w:p/></w:txbxContent></wps:txbx><wps:bodyPr/>"#);
    let mut e = editor(&format!(
        "<w:p><w:r><w:t>Text</w:t></w:r>{}{}</w:p>",
        anchor(72, 300, 150, 60, 1, false, 1, &full),
        anchor(250, 300, 150, 60, 2, false, 2, &empty)
    ));
    let ids = floats(&e);
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[0]))));
    assert!(!e.can_link_to(FloatId(ids[0])), "not to itself");
    assert!(e.begin_linking() == super::super::Outcome::Moved);
    assert_eq!(hover(&mut e, [300.0, 330.0]), super::super::input::Cursor::LinkReady, "the pouring pointer over the empty box");
    click(&mut e, 300.0, 330.0);
    assert!(!e.linking());
    let link = e.text_box_link().expect("linked");
    assert_eq!((link.index, link.count, link.next), (0, 2, Some(FloatId(ids[1]))));
    same_as_full(&mut e);
    let lines = page_lines(&e);
    assert!(lines.iter().any(|l| l.0.starts_with("P2") && l.1 > 250.0), "the text goes on in the second box: {lines:?}");
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let xml = crate::docx::package::read(&bytes, &[], "word/document.xml").expect("reads").expect("the main part");
    assert!(xml.contains(r#"<wps:txbx id="1">"#) && xml.contains(r#"<wps:linkedTxbx id="1" seq="1"/>"#), "{xml}");
    let mut again = Editor::open(&bytes, fonts()).expect("reopens");
    again.set_selection(Selection::Object(ObjectRef::Floating(FloatId(floats(&again)[0]))));
    assert_eq!(again.text_box_link().map(|l| l.count), Some(2));
    e.undo(true, fonts());
    assert_eq!(e.text_box_link(), None);
    let _ = e.execute(super::super::commands::Command::Redo(Default::default()), fonts());
    assert_eq!(e.text_box_link().map(|l| l.count), Some(2), "redone");
}

#[test]
fn releasing_an_inline_drag_after_the_selection_moved_changes_nothing() {
    use crate::layout::tests::shapes::PT;
    let shape = |id: u32| {
        let wsp = r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="W" cy="W"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="DDEEFF"/></a:solidFill></wps:spPr><wps:bodyPr/></wps:wsp>"#
            .replace('W', &(72 * PT).to_string());
        format!(
            r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{w}" cy="{w}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{id}" name="Shape {id}"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">{wsp}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#,
            w = 72 * PT
        )
    };
    let mut e = editor(&format!(r#"<w:p>{}<w:r><w:t xml:space="preserve">   </w:t></w:r>{}</w:p>"#, shape(1), shape(2)));
    let rects: Vec<[f64; 4]> = {
        let snap = e.snapshot();
        crate::layout::traverse::paint_items(&snap.pages()[0])
            .iter()
            .filter_map(|it| if let crate::layout::fragment::Item::Shape(s) = it { Some(s.rect) } else { None })
            .collect()
    };
    assert_eq!(rects.len(), 2);
    let b = click(&mut e, rects[1][0] + 2.0, rects[1][1] + 2.0);
    assert!(matches!(b, Selection::Object(ObjectRef::Inline { .. })), "{b:?}");
    click(&mut e, rects[0][0] + 2.0, rects[0][1] + 2.0);
    let frame = e.view(0..1).shapes.first().cloned().expect("a shape frame");
    let doc = |e: &Editor, p: [f64; 2]| e.stack().to_doc(0, p[0], p[1]).expect("on the stack");
    let (from, to) = (doc(&e, frame.rotate), doc(&e, [frame.rotate[0] + 100.0, frame.rotate[1] + 60.0]));
    e.pointer_input(PointerKind::Down, from.0, from.1, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
    assert!(e.dragging());
    let before = e.document().sections[0].blocks.clone();
    e.set_selection(b);
    e.pointer_input(PointerKind::Up, to.0, to.1, 0, Mods::default(), fonts());
    assert_eq!(e.document().sections[0].blocks, before, "neither shape turns");
}

/// A text box with `story` (`None`: linked into chain `chain` at `seq`), its story's chain id `chain` when given.
fn text_box(story: Option<&str>, chain: Option<u32>, seq: u32) -> String {
    let rect = sp(0, 0, 150, 60, "rect", "", "FFFFFF").replace(r#"<wps:cNvSpPr/>"#, r#"<wps:cNvSpPr txBox="1"/>"#);
    let id = chain.map(|c| format!(r#" id="{c}""#)).unwrap_or_default();
    let text = match story {
        Some(s) => format!(r#"<wps:txbx{id}><w:txbxContent>{s}</w:txbxContent></wps:txbx>"#),
        None => format!(r#"<wps:linkedTxbx{id} seq="{seq}"/>"#),
    };
    rect.replace("<wps:bodyPr/>", &format!("{text}<wps:bodyPr/>"))
}

fn boxes_editor(boxes: &[String]) -> Editor {
    let anchors: String = boxes
        .iter()
        .enumerate()
        .map(|(k, b)| anchor(72 + 160 * (k as i64 % 3), 300 + 80 * (k as i64 / 3), 150, 60, k as u32 + 1, false, k as u32 + 1, b))
        .collect();
    editor(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{anchors}</w:p>"))
}

#[test]
fn create_link_refuses_a_box_holding_a_bookmark_tracked_formatting_or_vertical_text() {
    let full = text_box(Some("<w:p><w:r><w:t>Story</w:t></w:r></w:p>"), None, 0);
    let marked = text_box(Some(r#"<w:p><w:bookmarkStart w:id="0" w:name="Here"/><w:bookmarkEnd w:id="0"/></w:p>"#), None, 0);
    let empty = text_box(Some("<w:p/><w:p/>"), None, 0);
    let tracked = text_box(
        Some(
            r#"<w:p><w:pPr><w:jc w:val="center"/><w:pPrChange w:id="9" w:author="A" w:date="2024-01-01T00:00:00Z"><w:pPr/></w:pPrChange></w:pPr></w:p>"#,
        ),
        None,
        0,
    );
    let vertical = text_box(Some("<w:p/>"), None, 0).replace("<wps:bodyPr/>", r#"<wps:bodyPr vert="vert"/>"#);
    let mut e = boxes_editor(&[full, marked, empty, tracked, vertical]);
    let ids = floats(&e);
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[0]))));
    assert!(!e.can_link_to(FloatId(ids[1])), "its bookmark would be lost");
    assert!(!matches!(e.link_text_box(FloatId(ids[1]), fonts()), super::super::Outcome::Changed(_)));
    assert!(e.can_link_to(FloatId(ids[2])), "empty paragraphs only");
    assert!(!e.can_link_to(FloatId(ids[3])), "its tracked formatting would be lost");
    assert!(!e.can_link_to(FloatId(ids[4])), "a vertical box: the chain's text would turn");
}

#[test]
fn create_link_takes_a_chain_id_drawingml_can_hold() {
    let head = text_box(Some("<w:p><w:r><w:t>Old</w:t></w:r></w:p>"), Some(65535), 0);
    let next = text_box(None, Some(65535), 1);
    let full = text_box(Some("<w:p><w:r><w:t>New</w:t></w:r></w:p>"), None, 0);
    let empty = text_box(Some("<w:p/>"), None, 0);
    let mut e = boxes_editor(&[head, next, full, empty]);
    let ids = floats(&e);
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[2]))));
    assert!(matches!(e.link_text_box(FloatId(ids[3]), fonts()), super::super::Outcome::Changed(_)));
    let c = chain_texts(&e);
    let made = c[2].2.expect("a chain id");
    assert!(made != 65535 && made <= u32::from(u16::MAX), "{c:?}");
    assert!(matches!(c[3].1, crate::drawing::ShapeText::Linked { chain, seq: 1 } if chain == made), "{c:?}");
}

#[test]
fn create_link_goes_on_after_the_last_boxs_place() {
    let head = text_box(Some("<w:p><w:r><w:t>Story</w:t></w:r></w:p>"), Some(1), 0);
    let b = text_box(None, Some(1), 8);
    let empty = text_box(Some("<w:p/>"), None, 0);
    let mut e = boxes_editor(&[head, b, empty]);
    let ids = floats(&e);
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[1]))));
    assert!(matches!(e.link_text_box(FloatId(ids[2]), fonts()), super::super::Outcome::Changed(_)));
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[2]))));
    let link = e.text_box_link().expect("in the chain");
    assert_eq!((link.index, link.count, link.previous), (2, 3, Some(FloatId(ids[1]))), "A, B, then C");
    // At the top of DrawingML's range, the chain is numbered again.
    let head = text_box(Some("<w:p><w:r><w:t>Story</w:t></w:r></w:p>"), Some(1), 0);
    let mut e = boxes_editor(&[head, text_box(None, Some(1), 65535), text_box(Some("<w:p/>"), None, 0)]);
    let ids = floats(&e);
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[1]))));
    assert!(matches!(e.link_text_box(FloatId(ids[2]), fonts()), super::super::Outcome::Changed(_)));
    let c = chain_texts(&e);
    assert!(
        matches!((&c[1].1, &c[2].1), (crate::drawing::ShapeText::Linked { seq: 1, .. }, crate::drawing::ShapeText::Linked { seq: 2, .. })),
        "{c:?}"
    );
}

#[test]
fn create_link_refuses_a_chain_with_a_member_that_cant_be_changed() {
    let sdt = |inner: &str| {
        format!(r#"<w:sdt><w:sdtPr><w:id w:val="5"/><w:lock w:val="sdtContentLocked"/></w:sdtPr><w:sdtContent>{inner}</w:sdtContent></w:sdt>"#)
    };
    let head = text_box(Some("<w:p><w:r><w:t>Story</w:t></w:r></w:p>"), Some(1), 0);
    let tail = text_box(None, Some(1), 65535);
    let empty = text_box(Some("<w:p/>"), None, 0);
    let at = |k: u32, b: &str| anchor(72 + 160 * i64::from(k), 300, 150, 60, k + 1, false, k + 1, b);
    let body = format!("<w:p>{}</w:p><w:p><w:r><w:t>Text</w:t></w:r>{}{}</w:p>", sdt(&at(0, &head)), at(1, &tail), at(2, &empty));
    let mut e = editor(&body);
    let ids = floats(&e);
    let before = chain_texts(&e);
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[1]))));
    assert!(!e.can_link_to(FloatId(ids[2])), "renumbering would write the locked head");
    assert!(!matches!(e.link_text_box(FloatId(ids[2]), fonts()), super::super::Outcome::Changed(_)));
    assert_eq!(chain_texts(&e), before);
}

#[test]
fn create_link_refuses_a_text_box_with_no_story_to_own_the_chain() {
    let rect = sp(0, 0, 150, 60, "rect", "", "FFFFFF").replace(r#"<wps:cNvSpPr/>"#, r#"<wps:cNvSpPr txBox="1"/>"#);
    let mut e = boxes_editor(&[rect, text_box(Some("<w:p/>"), None, 0)]);
    let ids = floats(&e);
    let before = chain_texts(&e);
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[0]))));
    assert!(!e.can_link_to(FloatId(ids[1])));
    assert!(!matches!(e.link_text_box(FloatId(ids[1]), fonts()), super::super::Outcome::Changed(_)));
    assert_eq!(chain_texts(&e), before);
}

#[test]
fn create_link_refuses_a_box_whose_paragraph_mark_is_a_revision() {
    let full = text_box(Some("<w:p><w:r><w:t>Story</w:t></w:r></w:p>"), None, 0);
    let marked = text_box(Some(r#"<w:p><w:pPr><w:rPr><w:ins w:id="1" w:author="A"/></w:rPr></w:pPr></w:p><w:p/>"#), None, 0);
    let mut e = boxes_editor(&[full, marked, text_box(Some("<w:p/>"), None, 0)]);
    let ids = floats(&e);
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[0]))));
    assert!(!e.can_link_to(FloatId(ids[1])), "its inserted paragraph mark would be lost");
    assert!(e.can_link_to(FloatId(ids[2])));
}

#[test]
fn create_links_pointer_finds_a_turned_box_where_it_is_drawn() {
    let full = text_box(Some("<w:p><w:r><w:t>Story</w:t></w:r></w:p>"), None, 0);
    let turned = text_box(Some("<w:p/>"), None, 0).replacen("<a:xfrm", r#"<a:xfrm rot="5400000""#, 1);
    let mut e = boxes_editor(&[full, turned]);
    let ids = floats(&e);
    e.set_selection(Selection::Object(ObjectRef::Floating(FloatId(ids[0]))));
    assert!(e.begin_linking() == super::super::Outcome::Moved);
    // The second box, 150 × 60 at (232, 300), turned a quarter about its centre (307, 330).
    let at = |e: &mut Editor, x: f64, y: f64| {
        let (x, y) = e.stack().to_doc(0, x, y).expect("on the stack");
        e.link_target_at(x, y, fonts())
    };
    assert_eq!(at(&mut e, 307.0, 390.0), Some(FloatId(ids[1])), "below its unturned place, inside the turned box");
    assert_eq!(at(&mut e, 240.0, 330.0), None, "inside its unturned place only");
}

#[test]
fn releasing_an_inline_drag_after_its_picture_changed_changes_nothing_more() {
    use crate::layout::tests::shapes::PT;
    let wsp = r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="W" cy="W"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="DDEEFF"/></a:solidFill></wps:spPr><wps:bodyPr/></wps:wsp>"#
        .replace('W', &(72 * PT).to_string());
    let shape = format!(
        r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{w}" cy="{w}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="1" name="Shape 1"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">{wsp}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#,
        w = 72 * PT
    );
    let mut e = editor(&format!("<w:p>{shape}</w:p>"));
    let rect = {
        let snap = e.snapshot();
        crate::layout::traverse::paint_items(&snap.pages()[0])
            .iter()
            .find_map(|it| if let crate::layout::fragment::Item::Shape(s) = it { Some(s.rect) } else { None })
            .expect("the shape")
    };
    click(&mut e, rect[0] + 2.0, rect[1] + 2.0);
    let frame = e.view(0..1).shapes.first().cloned().expect("a shape frame");
    let doc = |e: &Editor, p: [f64; 2]| e.stack().to_doc(0, p[0], p[1]).expect("on the stack");
    let (from, to) = (doc(&e, frame.rotate), doc(&e, [frame.rotate[0] + 100.0, frame.rotate[1] + 60.0]));
    e.pointer_input(PointerKind::Down, from.0, from.1, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Move, to.0, to.1, 0, Mods::default(), fonts());
    assert!(e.dragging());
    assert!(matches!(e.set_object_title("Changed", fonts()), super::super::Outcome::Changed(_)));
    let before = e.document().sections[0].blocks.clone();
    e.pointer_input(PointerKind::Up, to.0, to.1, 0, Mods::default(), fonts());
    assert_eq!(e.document().sections[0].blocks, before, "the changed picture doesn't turn");
}
