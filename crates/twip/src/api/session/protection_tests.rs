//! Document protection and locks on every session path: services, pointer and host gestures, batches, the state

use crate::api::command::{
    Command, CommandId, DeleteOptions, DocumentProtection, EditableRange, MoveSelection, MoveUnit, ObjectSize, ProtectedPictureControls,
    ProtectionKind, ReplaceFont, StyleMetaChange, TableSize, ViewOptions,
};
use crate::api::engine::{Engine, SessionOptions};
use crate::api::error::ErrorCode;
use crate::api::event::Event;
use crate::api::fonts::FontLibrary;
use crate::api::input::{Key, Modifiers, Point, PointerButton, PointerKind};
use crate::api::record::Author;
use crate::api::services::{FindQuery, StyleOpen};
use crate::api::session::Session;
use crate::api::value::{Direction, Order, Points, StyleId};
use crate::docx::{Block, Inline};
use crate::editing::position::{Position, StoryId};
use crate::editing::selection::Selection;
use crate::editing::tests::{blank_body, texts, texts_ids};
use crate::layout::{ParaId, Unit};

fn engine() -> Engine {
    Engine::new(FontLibrary::system())
}

fn session(engine: &mut Engine) -> &mut Session {
    let id = engine.create_session(SessionOptions::default());
    engine.session(id).expect("a session")
}

fn opened<'a>(engine: &'a mut Engine, bytes: &[u8]) -> &'a mut Session {
    let s = session(engine);
    s.open(bytes).expect("opens");
    s
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name)).expect("the fixture")
}

fn run(t: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{t}</w:t></w:r>"#)
}

fn protect(s: &mut Session, kind: ProtectionKind, password: Option<&str>) {
    s.run(Command::SetDocumentProtection(DocumentProtection {
        kind: Some(kind),
        enforce: Some(true),
        password: password.map(Into::into),
        ..DocumentProtection::default()
    }))
    .expect("protects");
}

fn code<T: std::fmt::Debug>(r: Result<T, crate::api::error::Error>) -> Option<ErrorCode> {
    r.err().map(|e| e.code)
}

fn query(text: &str) -> FindQuery {
    FindQuery { text: text.into(), ..FindQuery::default() }
}

fn whole(text: &str) -> FindQuery {
    FindQuery { whole_words: Some(true), ..query(text) }
}

fn mv(by: MoveUnit, direction: Direction) -> Command {
    Command::MoveSelection(MoveSelection { by, direction, extend: None })
}

fn all_text(s: &Session) -> Vec<String> {
    texts(&s.editor)
}

/// "Hello survey world", then a second paragraph; "survey" is editable, and read-only protection is enforced.
fn editable_survey(engine: &mut Engine) -> &mut Session {
    let s = session(engine);
    s.run(Command::InsertText("Hello survey world".into())).expect("types");
    s.run(Command::InsertParagraph).expect("splits");
    s.run(Command::InsertText("Locked second".into())).expect("types");
    s.find(&query("survey"), Order::Next).expect("finds");
    s.run(Command::AddEditableRange(EditableRange::default())).expect("a range");
    protect(s, ProtectionKind::ReadOnly, None);
    s
}

/// The session laid out in a host of `w` by `h` CSS px.
fn hosted(s: &mut Session, w: u32, h: u32) {
    s.host().attach(|_| {});
    s.host().set_size(f64::from(w), f64::from(h), 1.0);
    let mut px = vec![0u8; (w * h * 4) as usize];
    s.host().frame_rgba(&mut px, w, h).expect("a frame");
}

fn press_drag(s: &mut Session, from: Point, by: (f64, f64)) {
    let mods = Modifiers::default();
    s.pointer(PointerKind::Move, from, PointerButton::Primary, 0, mods);
    s.pointer(PointerKind::Down, from, PointerButton::Primary, 1, mods);
    for k in 1..=8 {
        let t = f64::from(k) / 8.0;
        s.pointer(PointerKind::Move, Point { x: from.x + by.0 * t, y: from.y + by.1 * t }, PointerButton::Primary, 0, mods);
    }
    s.pointer(PointerKind::Up, Point { x: from.x + by.0, y: from.y + by.1 }, PointerButton::Primary, 0, mods);
}

#[test]
fn replace_all_and_replace_honour_read_only_protection() {
    let mut engine = engine();
    let s = opened(&mut engine, &blank_body(&format!("<w:p>{}</w:p>", run("the cat and the dog"))));
    protect(s, ProtectionKind::ReadOnly, None);
    assert_eq!(code(s.run(Command::InsertText("x".into()))), Some(ErrorCode::NotAllowed), "typing is refused");
    let revision = s.state().revision;
    for q in [query("the"), whole("the")] {
        assert_eq!(code(s.replace_all(&q, "X", None).map(|c| c.count)), Some(ErrorCode::NotAllowed), "{q:?}");
    }
    s.find(&query("cat"), Order::Next).expect("finds");
    assert_eq!(code(s.replace(&query("cat"), "X", None)), Some(ErrorCode::NotAllowed));
    assert_eq!((all_text(s)[0].as_str(), s.state().revision), ("the cat and the dog", revision));
}

#[test]
fn replace_all_honours_forms_protection_and_replaces_only_in_editable_ranges() {
    let mut engine = engine();
    let s = opened(&mut engine, &blank_body(&format!("<w:p>{}</w:p>", run("the cat the dog"))));
    protect(s, ProtectionKind::Forms, None);
    assert_eq!(code(s.replace_all(&whole("the"), "X", None).map(|c| c.count)), Some(ErrorCode::NotAllowed));
    assert_eq!(all_text(s)[0], "the cat the dog");

    let mut engine = self::engine();
    let s = editable_survey(&mut engine);
    s.run(Command::SelectAll).expect("selects");
    assert_eq!(s.replace_all(&query("e"), "E", None).map(|c| c.count).expect("replaces in the range"), 1);
    assert_eq!(all_text(s), ["Hello survEy world", "Locked second"]);
}

#[test]
fn replace_all_skips_locked_content_whatever_the_query() {
    let locked =
        r#"<w:p><w:sdt><w:sdtPr><w:lock w:val="sdtContentLocked"/></w:sdtPr><w:sdtContent><w:r><w:t>apple</w:t></w:r></w:sdtContent></w:sdt></w:p>"#;
    for q in [query("apple"), whole("apple")] {
        let mut engine = engine();
        let s = opened(&mut engine, &blank_body(locked));
        assert_eq!(s.replace_all(&q, "plum", None).map(|c| c.count).expect("nothing to refuse"), 0, "{q:?}");
        assert_eq!(s.find_all(&query("apple")).expect("finds").matches.len(), 1, "{q:?}: locked text was rewritten");
    }
}

#[test]
fn replace_all_types_the_replacement_whatever_the_query() {
    let count = |q: FindQuery| {
        let mut engine = engine();
        let s = opened(&mut engine, &blank_body(&format!("<w:p>{}</w:p>", run("apple"))));
        s.replace_all(&q, "x\ny", None).map(|c| c.count).expect("replaces");
        s.word_count(Default::default()).paragraphs
    };
    assert_eq!(count(query("apple")), 2);
    assert_eq!(count(whole("apple")), 2);
}

#[test]
fn the_style_dialog_honours_protection() {
    let mut engine = engine();
    let s = opened(&mut engine, &fixture("FX-STYLES.docx"));
    let model = s.dialog_open_style(StyleOpen::Modify(StyleId("Heading1".into()))).expect("opens");
    protect(s, ProtectionKind::ReadOnly, None);
    let revision = s.state().revision;
    assert_eq!(code(s.dialog_apply(model)), Some(ErrorCode::NotAllowed));
    assert_eq!(s.state().revision, revision);
}

#[test]
fn a_spelling_replacement_honours_protection() {
    use crate::api::engine::Dictionary;
    use crate::api::services::MenuAction;
    use crate::api::value::Bytes;
    let mut engine = engine();
    let s = opened(&mut engine, &fixture("FX-services.docx"));
    let dictionary = |f: &str| {
        Bytes(std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dictionaries").join(f)).expect("a dictionary"))
    };
    s.spelling()
        .add_dictionary(Dictionary { language: "en-US".into(), aff: dictionary("en_US.aff"), dic: dictionary("en_US.dic"), hyphenation: None })
        .expect("loads");
    protect(s, ProtectionKind::ReadOnly, None);
    assert_eq!(s.spelling().next_misspelling().map(|m| m.word).as_deref(), Some("Teh"));
    let (before, revision) = (all_text(s), s.state().revision);
    assert_eq!(code(s.invoke(MenuAction::ReplaceWord { with: "The".into() })), Some(ErrorCode::NotAllowed));
    assert_eq!((all_text(s), s.state().revision), (before, revision));
}

#[test]
fn a_pointer_drag_of_text_in_a_read_only_document_moves_nothing() {
    let mut engine = engine();
    let s = session(&mut engine);
    s.run(Command::InsertText("hello world and more words here".into())).expect("types");
    protect(s, ProtectionKind::ReadOnly, None);
    hosted(s, 900, 700);
    s.find(&query("world"), Order::Next).expect("finds");
    let f = s.host().frame_info();
    let y = f.caret_y + f.caret_height / 2.0;
    let (inside, away) = (Point { x: f.caret_x - 8.0, y }, Point { x: f.caret_x + 120.0, y });
    for (kind, at) in
        [(PointerKind::Down, inside), (PointerKind::Move, Point { x: inside.x + 20.0, y }), (PointerKind::Move, away), (PointerKind::Up, away)]
    {
        s.pointer(kind, at, PointerButton::Primary, 1, Modifiers::default());
    }
    assert_eq!(all_text(s), ["hello world and more words here"]);
}

fn one_float() -> String {
    use crate::layout::tests::shapes::{anchor, sp};
    let shape = sp(0, 0, 100, 100, "rect", "", "0000FF");
    format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(200, 300, 100, 100, 1, false, 1, &shape))
}

#[test]
fn a_pointer_drag_cannot_move_a_float_in_a_read_only_document() {
    let mut engine = engine();
    let s = opened(&mut engine, &crate::layout::tests::shapes::bytes(&one_float()).expect("a package"));
    s.host().attach(|_| {});
    s.host().set_size(2000.0, 4000.0, 1.0);
    let k = s.presentation.zoom() * 96.0 / 72.0;
    let floats = crate::editing::inspect::inspect(&mut s.editor, "geometry", true)["geometry"]["floats"].as_array().cloned().unwrap_or_default();
    let r: Vec<f64> = floats[0]["rect"].as_array().expect("a rect").iter().filter_map(serde_json::Value::as_f64).collect();
    let at = Point { x: (r[0] + r[2]) / 2.0 * k, y: (r[1] + r[3]) / 2.0 * k };
    s.pointer(PointerKind::Down, at, PointerButton::Primary, 1, Modifiers::default());
    s.pointer(PointerKind::Up, at, PointerButton::Primary, 0, Modifiers::default());
    protect(s, ProtectionKind::ReadOnly, Some("Example"));
    assert_eq!(code(s.run(Command::SetObjectSize(ObjectSize { width: Some(Points(50.0)), ..Default::default() }))), Some(ErrorCode::NotAllowed));
    let revision = s.state().revision;
    press_drag(s, at, (40.0, 40.0));
    assert_eq!(s.state().revision, revision);
}

#[test]
fn a_pointer_drag_cannot_move_a_table_border_in_a_read_only_document() {
    let cell = r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#;
    let grid = r#"<w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid>"#;
    let body = format!(
        r#"<w:p><w:r><w:t>Before</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>{grid}<w:tr>{cell}{cell}{cell}</w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#
    );
    let mut engine = engine();
    let s = opened(&mut engine, &crate::layout::tests::shapes::bytes(&body).expect("a package"));
    s.host().attach(|_| {});
    s.host().set_size(2000.0, 4000.0, 1.0);
    let border = |s: &mut Session| {
        let k = s.presentation.zoom() * 96.0 / 72.0;
        let rows = crate::editing::inspect::inspect(&mut s.editor, "geometry", true)["geometry"]["rows"].as_array().cloned().unwrap_or_default();
        let r: Vec<f64> = rows[0]["rect"].as_array().expect("a rect").iter().filter_map(serde_json::Value::as_f64).collect();
        let x = rows[0]["edges"].as_array().expect("edges").iter().find(|e| e[1] == 1).expect("edge 1")[0].as_f64().expect("x");
        Point { x: x * k, y: (r[1] + r[3]) / 2.0 * k }
    };
    let (at, revision) = (border(s), s.state().revision);
    press_drag(s, at, (30.0, 0.0));
    assert!(s.state().revision > revision, "unprotected, the drag edits the table");
    protect(s, ProtectionKind::ReadOnly, Some("Example"));
    let (at, revision) = (border(s), s.state().revision);
    press_drag(s, at, (-30.0, 0.0));
    assert_eq!(s.state().revision, revision);
}

#[test]
fn a_batch_checks_protection_where_each_command_runs() {
    let mut engine = engine();
    let s = editable_survey(&mut engine);
    let before = all_text(s);
    s.find(&query("survey"), Order::Next).expect("finds");
    let delete = Command::Delete(DeleteOptions { direction: Direction::Forward, unit: None });
    assert_eq!(code(s.batch(vec![mv(MoveUnit::Paragraph, Direction::Forward), Command::InsertText("x".into())], None)), Some(ErrorCode::NotAllowed));
    s.find(&query("survey"), Order::Next).expect("finds");
    assert_eq!(code(s.batch(vec![Command::SelectAll, delete], None)), Some(ErrorCode::NotAllowed));
    assert_eq!(all_text(s), before);
}

#[test]
fn a_batch_runs_a_command_the_commands_before_it_make_available() {
    let mut engine = engine();
    let s = session(&mut engine);
    let r = s.batch(vec![Command::InsertTable(TableSize { rows: 2, columns: 2 }), Command::SetCellShading(None)], None);
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn a_delete_at_an_editable_range_edge_is_refused() {
    for end in [true, false] {
        let mut engine = engine();
        let s = editable_survey(&mut engine);
        let before = all_text(s);
        s.find(&query("survey"), Order::Next).expect("finds");
        let direction = if end { Direction::Forward } else { Direction::Backward };
        s.run(mv(MoveUnit::Character, direction)).expect("collapses onto the edge");
        for unit in [None, Some(crate::api::command::DeleteUnit::Word)] {
            assert_eq!(code(s.run(Command::Delete(DeleteOptions { direction, unit }))), Some(ErrorCode::NotAllowed), "end {end} {unit:?}");
        }
        assert_eq!(all_text(s), before);
        // Inside the range, the same delete goes.
        s.run(mv(MoveUnit::Character, if end { Direction::Backward } else { Direction::Forward })).expect("moves in");
        s.run(Command::Delete(DeleteOptions { direction, unit: None })).expect("deletes in the range");
    }
}

#[test]
fn undo_cannot_take_back_protection() {
    let mut engine = engine();
    let s = session(&mut engine);
    s.run(Command::InsertText("a".into())).expect("types");
    protect(s, ProtectionKind::ReadOnly, Some("pw"));
    assert!(s.run(Command::Undo(Default::default())).is_err(), "undo accepted under enforced protection");
    assert!(s.state().review.protection.as_ref().is_some_and(|p| p.enforced));
    assert_eq!(all_text(s), ["a"]);
    assert!(s.state().modified, "the document stays modified");
}

#[test]
fn view_options_and_protected_picture_controls_survive_open() {
    let mut engine = engine();
    let s = session(&mut engine);
    let bytes = fixture("FX-PLACEHOLDER.docx");
    s.open(&bytes).expect("opens");
    let tag = s.editor.document().controls.values().find_map(|c| c.props.tag.clone()).expect("a tagged control");
    s.run(Command::SetViewOptions(ViewOptions { live_layout: Some(false), control_labels: Some(false), ..ViewOptions::default() })).expect("sets");
    s.run(Command::SetProtectedPictureControls(ProtectedPictureControls { tag_prefixes: vec![tag] })).expect("protects");
    let protected = |s: &Session| s.editor.document().controls.keys().any(|c| s.editor.control_protected(*c));
    assert!(protected(s));
    s.open(&bytes).expect("opens again");
    assert!(!s.state().view.live_layout && !s.state().view.control_labels, "the view options went back");
    assert!(protected(s), "the protected picture controls went");
}

#[test]
fn tab_moves_between_cells_when_read_only_or_protected() {
    for protected in [false, true] {
        let mut engine = engine();
        let s = session(&mut engine);
        s.run(Command::InsertTable(TableSize { rows: 1, columns: 2 })).expect("a table");
        let cell = |s: &Session| s.state().selection.table.as_ref().map(|t| (t.row, t.column));
        s.run(Command::MoveToCell(Order::Previous)).ok();
        assert_eq!(cell(s), Some((0, 0)));
        if protected {
            protect(s, ProtectionKind::ReadOnly, None);
        } else {
            s.set_read_only(true);
        }
        s.key(Key::Tab, Modifiers::default());
        assert_eq!(cell(s), Some((0, 1)), "protected {protected}");
        // Past the last cell, a row would be added.
        let rows = |s: &Session| s.state().selection.table.as_ref().map(|t| t.rows);
        s.key(Key::Tab, Modifiers::default());
        assert_eq!(rows(s), Some(1), "protected {protected}");
    }
}

#[test]
fn formatting_protection_refuses_locked_character_and_table_styles_and_replace_font() {
    let mut engine = engine();
    let s = opened(&mut engine, &fixture("FX-STYLES-LOCKED.docx"));
    for id in ["Emph", "GridAccent", "Heading1"] {
        s.run(Command::SetStyleMeta(StyleMetaChange { id: StyleId(id.into()), locked: Some(true), ..StyleMetaChange::default() })).expect("locks");
    }
    assert_eq!(code(s.run(Command::SetParagraphStyle(StyleId("Heading1".into())))), Some(ErrorCode::NotAllowed));
    assert_eq!(code(s.run(Command::ApplyCharacterStyle(Some(StyleId("Emph".into()))))), Some(ErrorCode::NotAllowed));
    let font = ReplaceFont { from: "Calibri".into(), to: "Arial".into(), scope: Default::default(), bold: None, italic: None };
    assert_eq!(code(s.run(Command::ReplaceFont(font))), Some(ErrorCode::NotAllowed));
    s.run(Command::InsertTable(TableSize { rows: 1, columns: 1 })).ok();
    assert_eq!(code(s.run(Command::SetTableStyle(StyleId("GridAccent".into())))), Some(ErrorCode::NotAllowed));
}

#[test]
fn a_new_author_rebuilds_the_state() {
    let mut engine = engine();
    let changes = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let sink = changes.clone();
    engine.set_listener(move |_, e| {
        if matches!(e, Event::StateChanged(_)) {
            sink.set(sink.get() + 1);
        }
    });
    let s = session(&mut engine);
    s.run(Command::InsertText("ab".into())).expect("types");
    s.run(Command::SelectAll).expect("selects");
    s.run(Command::AddEditableRange(EditableRange { editor: Some("alice".into()), group: None })).expect("a range");
    s.set_author(Author { name: "alice".into(), ..Author::default() });
    protect(s, ProtectionKind::ReadOnly, None);
    let in_range = |s: &Session| s.state().review.protection.as_ref().is_some_and(|p| p.in_editable_range);
    assert!(in_range(s));
    let before = changes.get();
    s.set_author(Author { name: "bob".into(), ..Author::default() });
    assert!(changes.get() > before, "no state change");
    assert!(!in_range(s), "bob may not edit alice's range");
}

#[test]
fn a_formatted_paste_into_a_protected_form_field_is_text() {
    let mut engine = engine();
    let data = {
        let s = opened(&mut engine, &blank_body(&format!("<w:p>{}</w:p>", run("one"))));
        s.run(Command::SelectAll).expect("selects");
        s.run(Command::SetBold(true)).expect("bolds");
        s.copy(false).expect("copies")
    };
    let field = format!(r#"<w:sdt><w:sdtPr><w:id w:val="3"/><w:richText/></w:sdtPr><w:sdtContent><w:p>{}</w:p></w:sdtContent></w:sdt>"#, run("ab"));
    let s = opened(&mut engine, &blank_body(&field));
    let para = texts_ids(&s.editor)[0];
    s.editor.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para, Unit(1))));
    protect(s, ProtectionKind::Forms, None);
    assert_eq!(code(s.run(Command::Paste(Default::default()))), Some(ErrorCode::NotAllowed), "the formatted paste is refused");
    s.paste(&data, false);
    let p = s.editor.index().para(s.editor.document(), para).expect("the paragraph");
    assert_eq!(texts(&s.editor), ["a one b"], "the text went in, with the spaces a paste adds");
    assert!(p.inlines.iter().all(|i| !matches!(i, Inline::Text(_, r) if r.bold)), "the paste kept its formatting");
}

#[test]
fn a_host_drop_checks_protection_at_the_drop_point() {
    let mut engine = Engine::new(FontLibrary::system());
    let s = opened(&mut engine, &blank_body(&format!("<w:p>{}</w:p><w:p>{}</w:p>", run("Protected text"), run("Open text"))));
    hosted(s, 800, 600);
    let para = |s: &Session, n: usize| {
        let Block::Para(p) = &s.editor.document().sections[0].blocks[n] else { panic!("a paragraph") };
        ParaId(p.id)
    };
    let (p0, p1) = (para(s, 0), para(s, 1));
    s.editor.select_range(p1, Unit(0), Unit(9), &s.fonts.fonts());
    s.run(Command::AddEditableRange(EditableRange::default())).expect("editable");
    protect(s, ProtectionKind::ReadOnly, None);
    hosted(s, 800, 600);
    let point_in = |s: &mut Session, id: ParaId| {
        let g = s.editor.page_geometries().into_iter().find(|(p, _)| *p == 0).map(|(_, g)| g).expect("page 0");
        let l = g.lines.iter().find(|l| l.shows(id)).expect("a line");
        let x = l.stops.iter().find(|st| st.para == id && st.unit == Unit(1)).expect("a stop").x;
        let (dx, dy) = s.editor.stack().to_doc(0, x, (l.top + l.bottom) / 2.0).expect("on the stack");
        Point { x: dx * 96.0 / 72.0, y: dy * 96.0 / 72.0 }
    };
    let (protected_at, open_at) = (point_in(s, p0), point_in(s, p1));
    s.editor.select_range(p1, Unit(2), Unit(2), &s.fonts.fonts());
    assert_eq!(code(s.host().drop(protected_at, Command::InsertText("X".into()))), Some(ErrorCode::NotAllowed), "a drop on protected text");
    assert_eq!(s.editor.selection().focus().and_then(|f| f.para_offset()), Some((p1, Unit(2))), "the refused drop moved the caret");
    s.editor.select_range(p0, Unit(2), Unit(2), &s.fonts.fonts());
    let r = s.host().drop(open_at, Command::InsertText("Y".into()));
    assert!(r.is_ok(), "a drop into an editable range: {:?}", r.err());
    assert_eq!(all_text(s), ["Protected text", "OYpen text"]);
}

#[test]
fn autocorrect_does_not_rewrite_protected_text() {
    let body = format!(r#"<w:p>{}<w:permStart w:id="1" w:edGrp="everyone"/>{}<w:permEnd w:id="1"/></w:p>"#, run("teh"), run(" x"));
    let mut engine = Engine::new(FontLibrary::system_recommended());
    let s = opened(&mut engine, &blank_body(&body));
    let para = texts_ids(&s.editor)[0];
    s.editor.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para, Unit(3))));
    protect(s, ProtectionKind::ReadOnly, Some("Example"));
    s.text(" ");
    assert!(all_text(s)[0].starts_with("teh "), "{:?}", all_text(s));
}

#[test]
fn protection_disables_the_commands_it_refuses() {
    let on = |s: &Session, id: CommandId| s.state().commands[&id].enabled;
    let mut engine = engine();
    let s = session(&mut engine);
    protect(s, ProtectionKind::ReadOnly, None);
    assert!(s.run(Command::SetBold(true)).is_err());
    assert!(!on(s, CommandId::SetBold) && !on(s, CommandId::InsertText));
    assert!(on(s, CommandId::SelectAll));

    let mut engine = self::engine();
    let s = session(&mut engine);
    protect(s, ProtectionKind::TrackedChanges, None);
    assert!(!on(s, CommandId::AcceptAllRevisions) && !on(s, CommandId::RejectAllRevisions));
    assert!(on(s, CommandId::InsertText));
}
