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
    e.execute(Command::Undo, fonts());
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
    e.execute(Command::Undo, fonts());
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
    e.execute(Command::Undo, fonts());
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
    e.execute(Command::Undo, fonts());
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
    e.execute(Command::Undo, fonts());
    assert_eq!(floats(&e).len(), 2, "one undo step");
    e.execute(Command::Redo, fonts());
    // Ungroup
    click(&mut e, 150.0, 350.0);
    let out = e.ungroup_objects(fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert_eq!(floats(&e).len(), 2);
    assert_eq!(e.selected_floats().len(), 2);
    assert_eq!(drawn_boxes(&mut e), was, "ungrouped, every shape where it was");
    same_as_full(&mut e);
    e.execute(Command::Undo, fonts());
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
    e.execute(Command::Undo, fonts());
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
