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
    r.e.execute(Command::Undo, fonts());
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
                lock_aspect: None,
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
    r.e.execute(Command::Undo, fonts());
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
    r.e.execute(Command::Undo, fonts());
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
        Command::FillControl(crate::api::command::FillControl { id, text: None, image: Some(crate::api::value::Bytes(png)), docx: None }),
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
