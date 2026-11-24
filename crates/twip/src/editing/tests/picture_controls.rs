//! Picture controls: geometry, protection, dragging

use serde_json::json;

use super::super::Editor;
use super::super::commands::Command;
use super::fonts;
use super::scenarios::Run;
use crate::api::state::Mode;
use crate::docx::{ControlId, ControlProps};

const FX: &str = "FX-PICTURE-CONTROLS.docx";

fn tag_of(e: &Editor) -> Option<String> {
    e.selected_picture_control().and_then(|c| e.document().controls.get(&c)).and_then(|k| k.props.tag.clone())
}

/// Clicks the drawn pictures until one selects the picture control tagged `tag`; its index.
fn pick(r: &mut Run, tag: &str) -> usize {
    let st = crate::editing::inspect::inspect(&mut r.e, "geometry", true);
    let n = st["geometry"]["pictures"].as_array().map_or(0, |a| a.len());
    (0..n)
        .find(|&k| {
            r.step(&json!({ "click": { "picture": k } }));
            tag_of(&r.e).as_deref() == Some(tag)
        })
        .unwrap_or_else(|| panic!("no drawn picture in the control tagged {tag}"))
}

fn control(e: &Editor, tag: &str) -> (ControlId, ControlProps) {
    let (c, k) = e.document().controls.iter().find(|(_, k)| k.props.tag.as_deref() == Some(tag)).unwrap_or_else(|| panic!("no control {tag}"));
    (*c, k.props.clone())
}

fn size(e: &Editor, tag: &str) -> (f64, f64) {
    let s = e.picture_control_size(control(e, tag).0).expect("a picture");
    (s.width.0, s.height.0)
}

fn handles(e: &mut Editor) -> bool {
    matches!(e.view(0..1).selection, crate::editing::view::SelectionView::Object(_, true))
}

fn document_xml(bytes: &[u8]) -> String {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
    s
}

fn design(fixture: &str, id: &str) -> Run {
    let mut r = Run::open(fixture, id);
    r.e.execute(Command::SetMode(Mode::Design), fonts());
    r
}

/// A picture control read from a document resizes by its handles in design mode: a corner keeps the aspect, a side
/// doesn't, each one undo step, and the control, its size and its saved extent follow.
#[test]
fn a_picture_controls_handles_resize_it() {
    let mut r = design(FX, "control-resize");
    let k = pick(&mut r, "Placeholder:Signature.X");
    assert!(handles(&mut r.e), "handles: {:?}", r.e.selection());
    let images = r.e.document().images.len();
    let depth = r.e.history().undo_depth();
    let (w0, h0) = size(&r.e, "Placeholder:Signature.X");
    r.step(&json!({ "drag": { "from": { "picture": k, "at": "bottom-right" }, "to": { "picture": k, "at": "bottom-right", "dx": 40.0 } } }));
    let (w, h) = size(&r.e, "Placeholder:Signature.X");
    assert!(w > w0 + 20.0 && (w / h - w0 / h0).abs() < 0.05, "the corner keeps the aspect: {w}×{h}");
    assert_eq!((r.e.history().undo_depth(), r.e.document().images.len()), (depth + 1, images), "one step, the same picture");
    r.step(&json!({ "drag": { "from": { "picture": k, "at": "right" }, "to": { "picture": k, "at": "right", "dx": -60.0 } } }));
    let (w2, h2) = size(&r.e, "Placeholder:Signature.X");
    assert!(w2 < w - 30.0 && (h2 - h).abs() < 0.01, "a side doesn't: {w2}×{h2}");
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    let xml = document_xml(&bytes);
    let at = xml.find("Placeholder:Signature.X").expect("the control is saved");
    let extent = &xml[at..][xml[at..].find("<wp:extent").unwrap()..];
    let cx: f64 = extent.split('"').nth(1).unwrap().parse().unwrap();
    assert!((cx / 12700.0 - w2).abs() < 0.5, "the saved extent: {cx} for {w2}");
    let again = Editor::open(&bytes, fonts()).expect("reopens");
    let (_, props) = control(&again, "Placeholder:Signature.X");
    assert!(props.showing_placeholder && props.alias.as_deref() == Some("Signature"));
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(size(&r.e, "Placeholder:Signature.X"), (w, h));
}

/// Outside design mode, and when locked, a picture control can't be moved or resized.
#[test]
fn picture_control_geometry_follows_mode_and_lock() {
    let resize = |r: &mut Run| {
        r.e.execute(
            Command::SetObjectSize(crate::api::command::ObjectSize {
                width: Some(crate::api::value::Points(20.0)),
                height: Some(crate::api::value::Points(20.0)),
                ..Default::default()
            }),
            fonts(),
        )
    };
    let wrap = |r: &mut Run| r.e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::Square), fonts());
    let mut r = design(FX, "locked");
    let k = pick(&mut r, "Locked");
    assert!(!handles(&mut r.e), "a locked control has no handles");
    let before = size(&r.e, "Locked");
    r.step(
        &json!({ "drag": { "from": { "picture": k, "at": "bottom-right" }, "to": { "picture": k, "at": "bottom-right", "dx": 40.0, "dy": 30.0 } } }),
    );
    assert_eq!(size(&r.e, "Locked"), before, "a corner drag doesn't resize it");
    pick(&mut r, "Locked");
    assert!(matches!(resize(&mut r), super::super::Outcome::Unchanged(_)));
    assert!(matches!(wrap(&mut r), super::super::Outcome::Unchanged(_)));
    {
        let mode = Mode::Editing;
        let mut r = Run::open(FX, "not design");
        r.e.execute(Command::SetMode(mode), fonts());
        let k = pick(&mut r, "Placeholder:Signature.X");
        assert!(!handles(&mut r.e), "{mode:?}: no handles");
        let before = size(&r.e, "Placeholder:Signature.X");
        r.step(&json!({ "drag": { "from": { "picture": k, "at": "bottom-right" }, "to": { "picture": k, "at": "bottom-right", "dx": 40.0 } } }));
        assert_eq!(size(&r.e, "Placeholder:Signature.X"), before, "{mode:?}: not resized");
        pick(&mut r, "Placeholder:Signature.X");
        assert!(matches!(resize(&mut r), super::super::Outcome::Unchanged(_) | super::super::Outcome::Refused(_)), "{mode:?}");
    }
}

/// The paragraph and offset of the picture in the control tagged `tag`, and whether the control is inline around it.
fn place(e: &Editor, tag: &str) -> (crate::layout::ParaId, u32, bool) {
    let (c, _) = control(e, tag);
    let Some(crate::editing::selection::ObjectRef::Inline { para, offset }) = e.picture_of_control(c) else { panic!("{tag}: no inline picture") };
    let p = e.index().para(e.document(), para).unwrap();
    let inline = p.inlines.iter().any(|i| matches!(i, crate::docx::Inline::ControlStart(x) if *x == c));
    (para, offset.0, inline)
}

/// Dragging a picture control's picture takes the control along, as one undo step.
#[test]
fn dragging_a_picture_control_moves_its_control() {
    let mut r = design(FX, "control-drag");
    let (p0, o0, inline) = place(&r.e, "Placeholder:Signature.X");
    assert!(inline);
    let k = pick(&mut r, "Placeholder:Signature.X");
    let depth = r.e.history().undo_depth();
    let last = super::texts(&r.e).len() - 1;
    r.step(&json!({ "drag": { "from": { "picture": k }, "to": { "para": last, "offset": 5 } } }));
    let (p1, o1, inline) = place(&r.e, "Placeholder:Signature.X");
    assert!(p1 != p0 && inline, "moved with its control: {p0:?}:{o0} -> {p1:?}:{o1}");
    let (_, props) = control(&r.e, "Placeholder:Signature.X");
    assert_eq!((props.alias.as_deref(), props.showing_placeholder), (Some("Signature"), true));
    assert_eq!(r.e.document().controls.values().filter(|k| k.props.tag.as_deref() == Some("Placeholder:Signature.X")).count(), 1);
    assert_eq!(r.e.history().undo_depth(), depth + 1, "one step");
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(place(&r.e, "Placeholder:Signature.X").0, p0);

    let k = pick(&mut r, "Logo");
    let paras = super::texts(&r.e).len();
    r.step(&json!({ "drag": { "from": { "picture": k }, "to": { "para": last, "offset": 3 } } }));
    let (_, _, inline) = place(&r.e, "Logo");
    assert!(inline, "a block control moves as an inline one");
    assert_eq!(super::texts(&r.e).len(), paras, "the paragraph it leaves stays");
}

/// With its tag's prefix protected, a picture keeps its picture: in design mode Delete takes its whole control, in
/// editing mode nothing; unprotected, Delete takes the picture alone.
#[test]
fn protected_pictures_keep_their_picture() {
    let protect = |r: &mut Run, p: &[&str]| {
        r.e.execute(
            Command::SetProtectedPictureControls(crate::api::command::ProtectedPictureControls {
                tag_prefixes: p.iter().map(|x| x.to_string()).collect(),
            }),
            fonts(),
        )
    };
    let has = |e: &Editor, tag: &str| e.document().controls.values().any(|k| k.props.tag.as_deref() == Some(tag));
    let pictures = |e: &Editor| super::texts(e).iter().map(|t| t.matches('\u{FFFC}').count()).sum::<usize>();

    let mut r = design(FX, "protected");
    protect(&mut r, &["placeholder:"]);
    let n = pictures(&r.e);
    pick(&mut r, "Placeholder:Signature.X");
    r.e.execute(Command::PasteText("x".into()), fonts());
    assert_eq!(pictures(&r.e), n, "paste over doesn't replace it");
    let image = |e: &Editor| e.document().picture_controls().into_iter().find(|(_, c)| *c == control(e, "Placeholder:Signature.X").0).map(|(i, _)| i);
    let was = image(&r.e);
    pick(&mut r, "Placeholder:Signature.X");
    let png = super::open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    r.e.execute(Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(png), alt_text: None }), fonts());
    assert_eq!((image(&r.e), pictures(&r.e)), (was, n), "inserting a picture over it doesn't swap it");
    pick(&mut r, "Placeholder:Signature.X");
    r.step(&json!({ "key": "Delete" }));
    assert!(!has(&r.e, "Placeholder:Signature.X") && pictures(&r.e) == n - 1, "design mode: the whole control goes");
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert!(has(&r.e, "Placeholder:Signature.X") && pictures(&r.e) == n, "one undo step");

    let mut r = Run::open(FX, "protected editing");
    protect(&mut r, &["Placeholder:"]);
    pick(&mut r, "Placeholder:Signature.X");
    r.step(&json!({ "key": "Delete" }));
    r.e.execute(Command::PasteText("x".into()), fonts());
    assert!(has(&r.e, "Placeholder:Signature.X") && pictures(&r.e) == n, "editing mode: nothing");

    let mut r = design(FX, "unprotected");
    pick(&mut r, "Placeholder:Signature.X");
    r.step(&json!({ "key": "Delete" }));
    assert_eq!(pictures(&r.e), n - 1, "unprotected: the picture goes");
}

fn body_floats(e: &Editor) -> Vec<crate::docx::Float> {
    let mut out = Vec::new();
    for b in &e.document().sections[0].blocks {
        if let crate::docx::Block::Para(p) = b {
            out.extend(p.inlines.iter().filter_map(|i| if let crate::docx::Inline::Float(f, _) = i { Some((**f).clone()) } else { None }));
        }
    }
    out
}

/// A borderless, unfilled, zero-inset text box holding only a picture control is a floating picture control, read once
/// though a VML copy is beside it; the same box with a border is an ordinary text box.
#[test]
fn floating_picture_control_reads_once() {
    let mut r = Run::open(FX, "float-read");
    let floats = body_floats(&r.e);
    let kinds: Vec<Option<String>> =
        floats.iter().map(|f| f.picture_control(&r.e.document().controls).and_then(|c| r.e.document().controls[&c].props.tag.clone())).collect();
    assert_eq!(kinds, vec![Some("Placeholder:Secure.Y".to_string()), None], "the floating one and the boxed one");
    let all = crate::editing::elements::enumerate(r.e.document(), r.e.index(), &Default::default());
    let secure = all.iter().filter(|x| x.props.as_ref().is_some_and(|p| p.tag.as_deref() == Some("Placeholder:Secure.Y"))).count();
    assert_eq!(secure, 1, "one control, not the VML copy too");
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    let xml = document_xml(&bytes);
    assert_eq!((xml.matches("Placeholder:Secure.Y").count(), xml.contains("v:textbox")), (1, false), "saved without the VML copy");
    let again = Editor::open(&bytes, fonts()).expect("reopens");
    assert_eq!(body_floats(&again).iter().filter(|f| f.picture_control(&again.document().controls).is_some()).count(), 1);
    // A click selects the floating one as an object.
    r.step(&json!({ "click": { "float": 0 } }));
    assert_eq!(tag_of(&r.e).as_deref(), Some("Placeholder:Secure.Y"), "{:?}", r.e.selection());
}

/// Protected, in design mode, Delete takes a floating picture control with its box, so the text it wrapped
/// comes back; one undo step puts both back.
#[test]
fn a_protected_floating_picture_control_goes_with_its_box() {
    let mut r = design(FX, "float-protected-delete");
    r.e.execute(
        Command::SetProtectedPictureControls(crate::api::command::ProtectedPictureControls { tag_prefixes: vec!["Placeholder:".into()] }),
        fonts(),
    );
    let floats = body_floats(&r.e).len();
    let has = |e: &Editor| e.document().controls.values().any(|k| k.props.tag.as_deref() == Some("Placeholder:Secure.Y"));
    r.step(&json!({ "click": { "float": 0 } }));
    assert_eq!(tag_of(&r.e).as_deref(), Some("Placeholder:Secure.Y"), "{:?}", r.e.selection());
    r.step(&json!({ "key": "Delete" }));
    assert_eq!((body_floats(&r.e).len(), has(&r.e)), (floats - 1, false), "the box goes with the control");
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!((body_floats(&r.e).len(), has(&r.e)), (floats, true), "one undo step");
}

/// `insertControl` with `float` makes a floating picture control: saved as a bare borderless text box with no VML,
/// reopened as one, dragged and resized as a float (its picture keeping the box's size) and filled where it is.
#[test]
fn floating_picture_control_round_trip() {
    let insert = |r: &mut Run, v: serde_json::Value| r.e.execute(Command::InsertControl(serde_json::from_value(v).unwrap()), fonts());
    let mut r = design("FX-TEXT.docx", "float-insert");
    let refused = insert(&mut r, json!({ "kind": "plainText", "float": { "wrap": "square" } }));
    assert!(matches!(refused, super::super::Outcome::Refused(_)), "{refused:?}");
    let refused = insert(&mut r, json!({ "kind": "picture", "float": { "wrap": "inline" } }));
    assert!(matches!(refused, super::super::Outcome::Refused(_)), "{refused:?}");
    let float =
        json!({ "wrap": "square", "horizontal": { "relativeTo": "margin", "offset": 72 }, "vertical": { "relativeTo": "paragraph", "offset": 0 } });
    let out = insert(&mut r, json!({ "kind": "picture", "tag": "Placeholder:Float.Z", "float": float }));
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let f = body_floats(&r.e).into_iter().find(|f| f.picture_control(&r.e.document().controls).is_some()).expect("a floating picture control");
    assert!((f.h_offset - 72.0).abs() < 0.5 && f.h_relative == "margin", "placed: {} {}", f.h_relative, f.h_offset);
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    let xml = document_xml(&bytes);
    assert!(xml.contains("wps:txbx") && xml.contains(r#"lIns="0""#) && xml.contains("<a:noFill/>"), "the box");
    assert!(!xml.contains("mc:Fallback") && xml.matches("Placeholder:Float.Z").count() == 1, "one copy");
    let mut r = Run::from_bytes(&bytes, "float-reopen");
    r.e.execute(Command::SetMode(Mode::Design), fonts());
    let floats = |e: &Editor| body_floats(e).into_iter().filter(|f| f.picture_control(&e.document().controls).is_some()).collect::<Vec<_>>();
    let f = floats(&r.e).remove(0);
    let inner = |f: &crate::docx::Float| match f.text_box.as_ref().and_then(|t| t.blocks.first()) {
        Some(crate::docx::Block::Para(p)) => {
            p.inlines.iter().find_map(|i| if let crate::docx::Inline::Picture(p, _) = i { Some((p.w, p.h)) } else { None })
        }
        _ => None,
    };
    let k = body_floats(&r.e).iter().position(|x| x.id == f.id).unwrap();
    r.step(&json!({ "click": { "float": k } }));
    assert_eq!(tag_of(&r.e).as_deref(), Some("Placeholder:Float.Z"));
    r.step(&json!({ "drag": { "from": { "float": k, "at": "bottom-right" }, "to": { "float": k, "at": "bottom-right", "dx": 30.0 } } }));
    let g = floats(&r.e).remove(0);
    assert!(g.w > f.w + 10.0 && inner(&g) == Some((g.w, g.h)), "resized, the picture with it: {}×{} {:?}", g.w, g.h, inner(&g));
    r.step(&json!({ "drag": { "from": { "float": k }, "to": { "float": k, "dx": 60.0 } } }));
    let m = floats(&r.e).remove(0);
    assert!((m.h_offset - g.h_offset).abs() > 20.0, "moved: {} -> {}", g.h_offset, m.h_offset);
    let (c, _) = control(&r.e, "Placeholder:Float.Z");
    let png = super::open("FX-OBJ.docx").document().images.last().cloned().expect("an image");
    let id = crate::api::value::ElementId(crate::editing::elements::id_str(crate::docx::ElementId::Control(c)));
    let out = r.e.execute(
        Command::FillControl(crate::api::command::FillControl {
            id,
            text: None,
            image: Some(crate::api::value::Bytes(png)),
            docx: None,
            building_block: None,
            date: None,
        }),
        fonts(),
    );
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let n = floats(&r.e).remove(0);
    assert_eq!((n.h_offset, n.v_offset, n.w, n.h), (m.h_offset, m.v_offset, m.w, m.h), "filled where it is");
    assert!(!control(&r.e, "Placeholder:Float.Z").1.showing_placeholder);
}

/// Wrap Text turns a picture control into a floating one where it is, keeping the control, and back.
#[test]
fn picture_control_wrap_converts_both_ways() {
    let mut r = design(FX, "float-convert");
    let (c, _) = control(&r.e, "Placeholder:Signature.X");
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%"><rect width="100%" height="100%" fill="#3F51B5"/></svg>"##;
    let set = json!({
        "id": crate::editing::elements::id_str(crate::docx::ElementId::Control(c)),
        "placeholderImage": { "data": crate::api::value::base64(svg.as_bytes()), "mime": "image/svg+xml" },
    });
    r.e.execute(Command::SetControl(serde_json::from_value(set).unwrap()), fonts());
    let (_, props) = control(&r.e, "Placeholder:Signature.X");
    let size = size(&r.e, "Placeholder:Signature.X");
    pick(&mut r, "Placeholder:Signature.X");
    let depth = r.e.history().undo_depth();
    let out = r.e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::Square), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    let f = body_floats(&r.e).into_iter().find(|f| f.picture_control(&r.e.document().controls) == Some(c)).expect("floating now");
    assert_eq!((f.w, f.h), size);
    assert_eq!(control(&r.e, "Placeholder:Signature.X"), (c, props.clone()), "the same control");
    assert_eq!(r.e.history().undo_depth(), depth + 1, "one step");
    let out = r.e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::Inline), fonts());
    assert!(matches!(out, super::super::Outcome::Changed(_)), "{out:?}");
    assert!(body_floats(&r.e).iter().all(|f| f.picture_control(&r.e.document().controls) != Some(c)), "inline again");
    assert_eq!((control(&r.e, "Placeholder:Signature.X"), size_of(&r.e, c)), ((c, props), Some(size)));
    let mut r = design(FX, "float-locked");
    pick(&mut r, "Locked");
    assert!(matches!(r.e.execute(Command::SetObjectWrap(crate::api::value::ObjectWrap::Square), fonts()), super::super::Outcome::Unchanged(_)));
}

fn size_of(e: &Editor, c: ControlId) -> Option<(f64, f64)> {
    e.picture_control_size(c).map(|s| (s.width.0, s.height.0))
}

/// A selected picture control shows its label above it, as Word does, in editing mode too; `controlLabels` off, or
/// the control's own `w15:appearance` hidden, shows none.
#[test]
fn a_selected_picture_control_shows_its_label() {
    let mut r = Run::open(FX, "picture-label");
    pick(&mut r, "Placeholder:Signature.X");
    let (c, props) = control(&r.e, "Placeholder:Signature.X");
    let title = props.alias.clone().or(props.tag.clone()).expect("a title");
    let regions = r.e.view(0..2).regions;
    let g = regions.iter().find(|g| g.id == crate::editing::elements::id_str(crate::docx::ElementId::Control(c))).expect("its region");
    assert_eq!(g.title, title);
    let (_, first) = g.rects[0];
    assert!(first[3] - first[1] > 20.0, "the box is the picture's: {first:?}");
    assert_eq!(g.title_rect.1[3], first[1], "the tab sits on the box's top");
    r.e.control_labels = false;
    assert!(r.e.view(0..2).regions.is_empty(), "labels off");
    r.e.control_labels = true;
    let k = r.e.doc.controls.get_mut(&c).expect("the control");
    k.props.rest.push(("appearance".into(), r#"<w15:appearance w15:val="hidden"/>"#.into()));
    assert!(r.e.view(0..2).regions.iter().all(|g| g.title != title), "hidden by the control");
}

/// Dragging a picture control later in its own paragraph keeps the control around the picture, as one undo step.
#[test]
fn dragging_a_picture_control_later_in_its_paragraph_keeps_its_control() {
    let tag = "Placeholder:Signature.X";
    let mut r = design(FX, "control-drag-later");
    let (para, offset, inline) = place(&r.e, tag);
    assert!(inline);
    let idx = super::texts_ids(&r.e).iter().position(|p| *p == para).expect("its paragraph");
    let k = pick(&mut r, tag);
    let depth = r.e.history().undo_depth();
    r.step(&json!({ "drag": { "from": { "picture": k }, "to": { "para": idx, "offset": offset + 3 } } }));
    assert_eq!(r.e.history().undo_depth(), depth + 1, "one step");
    let (c, props) = control(&r.e, tag);
    assert_eq!(props.alias.as_deref(), Some("Signature"), "the control keeps its properties");
    let (p1, o1, inline) = place(&r.e, tag);
    assert!(p1 == para && inline && o1 == offset + 2, "moved inside its control: {offset} -> {o1}");
    let p = r.e.index().para(r.e.document(), para).unwrap();
    let pic = p.inlines.iter().position(|i| matches!(i, crate::docx::Inline::Picture(..))).expect("the picture");
    assert!(matches!(p.inlines[pic - 1], crate::docx::Inline::ControlStart(x) if x == c));
    assert!(matches!(p.inlines[pic + 1], crate::docx::Inline::ControlEnd(x) if x == c));
    r.e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(place(&r.e, tag), (para, offset, true));
}

fn fill_image(e: &mut Editor, c: ControlId, data: Vec<u8>) -> crate::editing::commands::Outcome {
    let id = crate::api::value::ElementId(crate::editing::elements::id_str(crate::docx::ElementId::Control(c)));
    e.execute(
        Command::FillControl(crate::api::command::FillControl {
            id,
            text: None,
            image: Some(crate::api::value::Bytes(data)),
            docx: None,
            building_block: None,
            date: None,
        }),
        fonts(),
    )
}

/// An image none of `not` is.
fn another_image(not: &[Vec<u8>]) -> Vec<u8> {
    let mut all = super::open("FX-OBJ.docx").document().images.clone();
    all.extend(super::open(FX).document().images.iter().cloned());
    all.into_iter().find(|i| !not.contains(i) && crate::docx::image::image_type(i).is_some()).expect("a different image")
}

fn image_of(e: &Editor, c: ControlId) -> Option<usize> {
    e.document().picture_controls().into_iter().find(|(_, k)| *k == c).map(|(i, _)| i)
}

/// Filling one of two picture controls that share an image fills it alone: the other keeps its image and size, in
/// the document and once saved.
#[test]
fn filling_a_picture_control_leaves_another_sharing_its_image() {
    let mut r = Run::open(FX, "shared-fill");
    let (a, b) = (control(&r.e, "Placeholder:Signature.X").0, control(&r.e, "Placeholder:Secure.Y").0);
    let shared = image_of(&r.e, a);
    assert!(shared.is_some() && shared == image_of(&r.e, b), "the two controls share an image: {shared:?}");
    let before = r.e.document().images[shared.unwrap()].clone();
    let (size_b, images) = (size_of(&r.e, b), r.e.document().images.len());
    let new = another_image(std::slice::from_ref(&before));
    let out = fill_image(&mut r.e, a, new.clone());
    assert!(matches!(out, crate::editing::commands::Outcome::Changed(_)), "{out:?}");
    assert_eq!(r.e.document().images.len(), images + 1, "an image of its own");
    assert_eq!(r.e.document().images[image_of(&r.e, a).unwrap()], new);
    assert_eq!(r.e.document().images[image_of(&r.e, b).unwrap()], before, "the other control's image is unchanged");
    assert_eq!(size_of(&r.e, b), size_b, "and its size");
    let saved = crate::docx::write::save(r.e.document()).expect("saves");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    let tags = |e: &Editor, tag: &str| image_of(e, control(e, tag).0).map(|i| e.document().images[i].clone());
    assert_eq!(tags(&again, "Placeholder:Signature.X"), Some(new), "saved with its new image");
    assert_eq!(tags(&again, "Placeholder:Secure.Y"), Some(before), "the other saved with its own");
}

/// The fixture's package with `body` for its body.
fn with_body(body: &str) -> Vec<u8> {
    use std::io::{Read, Write};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(FX);
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(std::fs::read(path).unwrap())).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "word/document.xml" {
            let x = String::from_utf8(data).unwrap();
            let (head, _) = x.split_once("<w:body>").unwrap();
            data = format!("{head}<w:body>{body}</w:body></w:document>").into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// A picture of the fixture's `rId10`, `cx` by `cy` EMU, its transform's attributes `xfrm`.
fn pic(id: u32, cx: u32, cy: u32, xfrm: &str) -> String {
    format!(
        r#"<pic:pic><pic:nvPicPr><pic:cNvPr id="{id}" name="image4.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId10"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm{xfrm}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>"#
    )
}

/// An anchored drawing `cx` by `cy` EMU of `graphic`, of the graphic data type `uri`.
fn anchored(id: u32, cx: u32, cy: u32, uri: &str, graphic: &str) -> String {
    format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="{id}" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="{id}" name="Picture {id}"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="{uri}">{graphic}</a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
    )
}

const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";

/// A picture control tagged `tag` holding `content`.
fn picture_control(tag: &str, id: u32, content: &str) -> String {
    format!(
        r#"<w:sdt><w:sdtPr><w:alias w:val="{tag}"/><w:tag w:val="{tag}"/><w:id w:val="{id}"/><w:picture /></w:sdtPr><w:sdtContent>{content}</w:sdtContent></w:sdt>"#
    )
}

fn changed(out: &crate::editing::commands::Outcome) -> bool {
    matches!(out, crate::editing::commands::Outcome::Changed(_))
}

/// A picture content control holding an anchored picture: the float is its picture, found and filled.
#[test]
fn an_anchored_picture_is_its_controls_picture() {
    let body = format!("<w:p>{}</w:p>", picture_control("Anchored", 9700, &anchored(7, 914400, 457200, PIC, &pic(7, 914400, 457200, ""))));
    let mut e = Editor::open(&with_body(&body), fonts()).expect("opens");
    let (c, _) = control(&e, "Anchored");
    let Some(crate::editing::selection::ObjectRef::Floating(f)) = e.picture_of_control(c) else { panic!("the control's float is found") };
    assert_eq!(e.picture_control_at(crate::editing::selection::ObjectRef::Floating(f)), Some(c), "the float is the control's");
    let was = e.document().images.clone();
    let k = image_of(&e, c).expect("its image");
    let out = fill_image(&mut e, c, another_image(&was));
    assert!(changed(&out), "fill: {out:?}");
    assert_ne!(e.document().images[image_of(&e, c).unwrap()], was[k], "its image is filled");
}

/// Filling an anchored picture control whose image another picture shows: its drawing shows the new image once
/// saved, the other the old one.
#[test]
fn a_filled_anchored_picture_saves_its_own_image() {
    let control = picture_control("Anchored", 9700, &anchored(7, 914400, 457200, PIC, &pic(7, 914400, 457200, "")));
    let body = format!("<w:p>{control}</w:p><w:p>{}</w:p>", anchored(8, 914400, 457200, PIC, &pic(8, 914400, 457200, "")));
    let mut e = Editor::open(&with_body(&body), fonts()).expect("opens");
    let c = self::control(&e, "Anchored").0;
    let was = e.document().images.clone();
    let before = was[image_of(&e, c).expect("its image")].clone();
    let new = another_image(&was);
    assert!(changed(&fill_image(&mut e, c, new.clone())));
    let blips: Vec<(Option<usize>, Option<usize>)> =
        body_floats(&e).iter().map(|f| (f.image, f.shape.as_deref().and_then(|s| s.pic()).and_then(|p| p.blip.image))).collect();
    assert!(blips[0].0 != blips[1].0 && blips.iter().all(|(a, b)| a == b), "each drawing's picture shows its own image: {blips:?}");
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    let shown: Vec<Vec<u8>> = body_floats(&again).iter().filter_map(|f| f.image.map(|k| again.document().images[k].clone())).collect();
    assert_eq!(shown, vec![new, before], "the control's picture, then the other");
}

/// Filling a picture control whose image a group's picture shows leaves the group's picture its image.
#[test]
fn filling_a_picture_control_leaves_a_grouped_picture_its_image() {
    let group = format!(
        r#"<wpg:wgp xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/><a:chOff x="0" y="0"/><a:chExt cx="914400" cy="457200"/></a:xfrm></wpg:grpSpPr>{}</wpg:wgp>"#,
        pic(9, 914400, 457200, "")
    );
    let control = picture_control("Anchored", 9700, &anchored(7, 914400, 457200, PIC, &pic(7, 914400, 457200, "")));
    let uri = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
    let body = format!("<w:p>{control}</w:p><w:p>{}</w:p>", anchored(8, 914400, 457200, uri, &group));
    let mut e = Editor::open(&with_body(&body), fonts()).expect("opens");
    let c = self::control(&e, "Anchored").0;
    let k = image_of(&e, c).expect("its image");
    let grouped = |e: &Editor| body_floats(e).iter().flat_map(|f| f.parts.iter().filter_map(|p| p.image)).collect::<Vec<_>>();
    assert_eq!(grouped(&e), vec![k], "the group's picture shares the control's image");
    let was = e.document().images.clone();
    assert!(changed(&fill_image(&mut e, c, another_image(&was))));
    let g = grouped(&e);
    assert_eq!(g.len(), 1);
    assert_eq!(e.document().images[g[0]], was[k], "the grouped picture keeps its image");
}

/// A white 24-bit BMP of `w` × `h` pixels.
fn bmp(w: u32, h: u32) -> Vec<u8> {
    let size = 54 + (w * 3).next_multiple_of(4) * h;
    let mut b = b"BM".to_vec();
    for v in [size, 0, 54, 40, w, h] {
        b.extend(v.to_le_bytes());
    }
    b.extend(1u16.to_le_bytes());
    b.extend(24u16.to_le_bytes());
    b.resize(size as usize, 0);
    b[54..].fill(255);
    b
}

/// A quarter-turned anchored picture fits a new image in its own, unturned width and height.
#[test]
fn a_quarter_turned_picture_control_fits_its_image_unturned() {
    // A 120 × 60 pt picture turned a quarter: its frame on the page is 60 × 120.
    let (w, h) = (120 * 12700, 60 * 12700);
    let turned = anchored(7, w, h, PIC, &pic(7, w, h, r#" rot="5400000""#));
    let body = format!("<w:p>{}</w:p>", picture_control("Turned", 9700, &turned));
    let mut e = Editor::open(&with_body(&body), fonts()).expect("opens");
    let c = control(&e, "Turned").0;
    let frame = |e: &Editor| body_floats(e).first().map(|f| (f.w, f.h, f.rot_swap)).expect("the float");
    assert_eq!(frame(&e), (60.0, 120.0, true));
    assert!(changed(&fill_image(&mut e, c, bmp(40, 20))));
    assert_eq!(frame(&e), (60.0, 120.0, true), "a 2:1 image fits as it is");
}

/// Pasting a picture over a picture control's picture fills the control, protected or not, fitted to its box, as one
/// undo step.
#[test]
fn pasting_a_picture_over_a_picture_control_fills_it() {
    for protected in [true, false] {
        let mut r = Run::open(FX, "paste-fill");
        if protected {
            r.e.execute(
                Command::SetProtectedPictureControls(crate::api::command::ProtectedPictureControls { tag_prefixes: vec!["Placeholder:".into()] }),
                fonts(),
            );
        }
        let c = control(&r.e, "Placeholder:Signature.X").0;
        let (was, size, count) = (image_of(&r.e, c), size_of(&r.e, c), super::texts(&r.e).concat().matches('\u{FFFC}').count());
        pick(&mut r, "Placeholder:Signature.X");
        let png = crate::editing::wrap_points::tests::png(40, 10, &[90u8; 40 * 10 * 4]);
        let out = r.e.paste_clip(crate::editing::paste::Clip { image: Some(png.clone()), ..Default::default() }, None, fonts());
        assert!(changed(&out), "protected {protected}: {out:?}");
        let now = image_of(&r.e, c);
        assert!(now.is_some() && now != was && r.e.document().images[now.unwrap()] == png, "protected {protected}: the control shows it");
        assert_eq!(super::texts(&r.e).concat().matches('\u{FFFC}').count(), count, "the one picture");
        let s = size_of(&r.e, c).expect("a size");
        let w = size.expect("a size");
        let k = (w.0 / 40.0).min(w.1 / 10.0);
        assert!((s.0 - 40.0 * k).abs() <= 0.05 && (s.1 - 10.0 * k).abs() <= 0.05, "fitted to its box, its shape kept: {s:?} in {w:?}");
        assert_eq!(r.e.history().next_undo(), Some("Paste"));
        r.e.execute(Command::Undo(Default::default()), fonts());
        assert_eq!((image_of(&r.e, c), size_of(&r.e, c)), (was, size), "one undo step");
    }
}
